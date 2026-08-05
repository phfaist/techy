# Defining a custom language

A language, to techy, is an implementation of the [`Lang`](crate::core::Lang)
trait: one compile-time bundle of vocabulary types, hooks, and defaults that
every core type takes as its single `L: Lang` parameter. A minimal language
is a unit struct with the associated types filled in — every `Lang` method
except one has a working default — and each additional feature is one more
customization point filled in. This chapter groups those customization
points by feature and says what each is *for*; the contracts and obligations
live on [`Lang`](crate::core::Lang)'s own API page and its per-method
documentation, which is the reference throughout.

You rarely start from a blank `Lang`: there are two standard starting
points — the all-defaults [`TrivialLang`](crate::core::TrivialLang) for
experiments, and the `latexlike` language family for anything LaTeX-shaped
— described [at the end of this chapter](#the-two-starting-points) once the
pieces are on the table. This chapter builds on [The parsing model](crate::guide::parsing_model)
(what drivers, specs, and construct parsers do at parse time).

## Vocabularies: group classes and callable types

Two of `Lang`'s associated types name the language's syntactic taxonomy,
and both are **closed per language** — known when the `Lang` is written,
typically small enums:

- [`GroupTypeId`](crate::core::Lang::GroupTypeId) classifies groups (the
  preset: content vs. math vs. verbatim), fully detached from delimiter
  spellings. Which delimiter *pairs* exist, and which class each maps to,
  is runtime data — [`GroupRule`](crate::core::GroupRule) values in the
  state's token rules, extensible mid-parse; only the class vocabulary is
  fixed.
- [`CallableTypeId`](crate::core::Lang::CallableTypeId) names the
  invocation *forms* (the preset: macro, environment, specials). New forms
  are never registered at runtime — new *callables* are, through the
  [scope stack](crate::guide::concepts_overview#scopes-and-packages).

A vocabulary that wants generic tooling to enumerate its values opts into
[`ClosedVocabulary`](crate::core::ClosedVocabulary); it is deliberately not
required (the `u32` defaults of test languages have no value list).

For a language joining the latexlike family, the **role traits** connect
your vocabulary types to the preset's machinery: they add constructors and
predicates *on your own types* — which value is the content group, which
values count as math ([`LatexlikeGroupType`](crate::latexlike::LatexlikeGroupType),
with its documented split between parse behavior and presentation), which
value plays the macro/environment/specials role
([`LatexlikeCallableType`](crate::latexlike::LatexlikeCallableType)), and so
on — so every generic preset component can operate over any family member.
Each role trait's page states its coherence contract.

## Modes

The parsing mode is the third closed vocabulary:
[`ModeId`](crate::core::Lang::ModeId) names the mode a state is in (the
preset: text and math), stored as first-class state data. The division of
labor is documented on the type: deltas *initiate* mode changes (the
[`mode`](crate::core::ParsingStateDelta::mode) override channel), and the
language *interprets* them in
[`finalize_transition`](crate::core::Lang::finalize_transition) — adjusting
rules on entry, disabling features. Definition visibility may key on the
mode (a math-only package), as may any content-interpretation decision. The
family-side role trait is
[`LatexlikeMode`](crate::latexlike::LatexlikeMode), deliberately minimal:
the preset never conjures a "text mode" — leaving math restores an actual
enclosing context.

## Token rules and specials recognition

Tokenization is data, not code: [`TokenRules`](crate::core::TokenRules) —
stored in the parsing state, so all of it can change mid-parse — declares
the whitespace set, group delimiter pairs, command escape rules, comment
markers, forbidden characters, and a per-feature `enable_*` gate. The type's
documentation records the detection priority and the two distinct spellings
of "off" (gate off = scoped, data preserved; empty data = the language has
no such feature). A language whose tokenization *behavior* — not just data —
differs implements the [`TokenReader`](crate::core::TokenReader) trait
instead. The preset's canonical rules
([`default_token_rules`](crate::latexlike::default_token_rules)) are the
worked example.

**Specials** — callables triggered by plain character sequences (`~`,
`--`) — are recognized by two `Lang` hooks, and here sits a documented
silent trap: **the two hooks must be wired together**.
[`scan_specials`](crate::core::Lang::scan_specials) answers "is a trigger
at this position?" (recognition and resolution in one call), but it is
*only consulted* when the current character is in the set returned by
[`specials_trigger_chars`](crate::core::Lang::specials_trigger_chars) —
computed once per frozen state, as the hot-path filter. A first character
missing from that set means the trigger **silently never fires**: no error,
no diagnostic. The scan hook's documentation adds the second quiet
obligation: specials have the *lowest* recognition precedence, so a trigger
overlapping a group delimiter, escape character, or comment start also
silently never fires — the `Lang` author is the one who can create, and
must avoid, such a collision. The preset wires both hooks to the scope
stack (providers advertise their triggers), so registered specials come
with their trigger characters automatically.

## Extension types: attaching custom information

A language can attach its own data to parsed material without touching the
structural node kinds — a group with custom data is still a group to all
generic tooling. The attachment points:

- **Per node**: the [`NodeExts`](crate::core::Lang::NodeExts) bundle
  ([`NodeExtTypes`](crate::core::NodeExtTypes)) declares one ext type
  carried by every node, plus one for parsed-argument records and one for
  parsed-slot records. The governing rule, documented on the bundle:
  **population is initialization** — an ext value is minted exactly once,
  at creation, by the party with the knowledge, and there is no
  "default now, populate later" state anywhere.
- **The node ext mint**:
  [`make_node_ext`](crate::core::Lang::make_node_ext) is the language's
  one chance to compute per-node data, with the node's parts in view — and
  it is the **only required `Lang` method** (a no-ext language writes the
  empty one-liner). Its page documents exactly what the hook can see (the
  descent-only view of the staged children; deliberately no parent access).
- **Per argument and per slot**: the argument ext is minted by the
  [`ArgumentParser`](crate::core::constructs::ArgumentParser) that parsed
  the argument; the slot ext by the invocation composition that mints the
  slot record. The preset claims the slot ext for its body marker (the
  [`BodySlotExt`](crate::core::node::BodySlotExt) mechanism behind
  [`NodeRef::body`](crate::core::node::NodeRef::body)).
- **Invocation spelling**: the
  [`InvocationSyntax`](crate::core::Lang::InvocationSyntax) payload records
  the trigger-spelling facts of each callable invocation (escape character,
  post-space, environment scaffolding) in the language's own form. As its
  documentation puts it, this channel is what makes *recomposition
  accuracy the language's choice* — byte-exact re-emission is possible
  exactly to the extent the language records spelling facts here. `()`
  records nothing; construction from a resolved invocation is the opt-in
  [`FromInvocation`](crate::core::constructs::FromInvocation) contract that
  the standard staging sites use.

## Language state and `finalize_transition`

[`StateExt`](crate::core::Lang::StateExt) is the language's own slice of
the parsing state — typed feature flags and settings, not an `Any` map. Its
documentation carries one hard rule: a plain value type, **no interior
mutability** — states are frozen at construction, and their derived caches
are computed at freeze time.

[`finalize_transition`](crate::core::Lang::finalize_transition) is the
transition customizer: it runs exactly once per
[`derived()`](crate::core::ParsingState::derived) call, after the delta's
overrides are applied and before the new state freezes. Cross-cutting rules
live here and nowhere else — mode-dependent rule adjustments, invariants
over the language's state ext. Its documentation pins the obligations that
make the machinery sound: it must be a pure function of
`(new, prev, events)`; mode changes are interpreted here (the applied
override is the signal — compare `prev.mode()` with the new mode); and
anything history-shaped belongs in the driver's
[`observe_transition`](crate::core::ParseDriver::observe_transition)
instead, which fires on every transition while the customizer runs once per
unique derivation. Semantic [events](crate::core::Lang::Event) come in the
two documented classes: context-free events reach the customizer wherever
the delta is applied; context-dependent ones (the preset's exit-math
restore) are lowered by the driver inside a parse and must be *refused*
loudly if they ever reach the bare customizer.

**Replay granularity.** One consequence of deltas being mergeable values
deserves a note for order-sensitive customizers. A content run's sibling
after-effects are exported as **one merged delta**
([`NodesOutcome::after_effects`](crate::core::constructs::NodesOutcome)):
later field overrides win, scope ops and context-free events concatenate in
application order. When a construct forwards that merged record as its own
after-effect — as the shipped `\input` state persistence documents doing
([`InputMacroSpec`](crate::latexlike::InputMacroSpec), with state
persistence enabled) — the caller applies it in a *single* derivation:
`finalize_transition` sees one transition carrying the merged delta, not
one call per original operation. A customizer that reacts to intermediate
values (a mode that was entered and left again inside the included file)
sees only the net result; scope ops arrive in order, but field overrides
arrive already collapsed. Design customizers against the merged form — the
delta type's documentation
([`ParsingStateDelta`](crate::core::ParsingStateDelta)) is explicit that
deltas are values built to be merged and applied to bases their producer
never saw.

## The driver

Everything that only runs *while a parse is driven* is not on `Lang` but on
the language's driver type ([`Lang::Driver`](crate::core::Lang::Driver), an
implementation of [`ParseDriver`](crate::core::ParseDriver)) — an instance,
so behavior can carry configuration (a recovery policy) that static hooks
could not. The trait's page groups its five concerns: recovery policy,
parse-time hooks (command resolution, paragraph-break emission, diagnostic
refinement, transition observation, event lowering), source resolution,
the group descent-delta channel, and construct provision. Every method has
a working default, so `impl ParseDriver<MyLang> for MyDriver {}` is a
complete driver; override what your language needs.

**Command resolution** is the hook a command-bearing language cannot leave
defaulted — the core cannot know which of your callable types commands
resolve under. The canned driver
([`StdParseDriver`](crate::core::StdParseDriver)) makes it a plug-in
strategy ([`CommandResolver`](crate::core::CommandResolver)): `()` resolves
nothing (right for test languages and languages without commands — and its
failure detail says so, rather than leaving a bare "cannot resolve"), and
[`ScopesCommandResolver`](crate::core::specs::ScopesCommandResolver)
resolves every command through the state's scope stack under one fixed
callable type — the standard shape, packaged from
[`resolve_command_in_scopes`](crate::core::specs::resolve_command_in_scopes).
A language with several command-shaped callable types, or non-scope-stack
resolution, writes its own resolver or its own driver — the documented
normal path.

## The two starting points

**Experiments: `TrivialLang`.** For exercising the machinery — tests,
prototypes of custom construct parsers, learning the engine —
`impl TrivialLang for MyLang {}` yields a complete
[`Lang`](crate::core::Lang) with every type defaulted and every behavior
neutral (no modes, no exts, a driver that resolves nothing; the seed state
has every syntax feature off, so content is plain characters until you
derive rules into it):

```rust
use techy::core::{Language, ParsingState, StdParseDriver, TrivialLang};
use techy::error::Recovery;

#[derive(Debug, Clone, Copy)]
struct MyLang;
impl TrivialLang for MyLang {}

let language: Language<MyLang> = Language::new(
    StdParseDriver::new(Recovery::Strict, ()),
    ParsingState::lang_initial(),
);
let result = language.parse("hello").unwrap();
assert_eq!(result.tree.root().child(0).unwrap().chars(), Some("hello"));
```

The blanket implementation makes `TrivialLang` and a direct `Lang`
implementation mutually exclusive: the first real vocabulary type or hook
means implementing [`Lang`](crate::core::Lang) yourself.

**Anything LaTeX-shaped: join the `latexlike` family.** The preset is not a
monolith: [`Latexlike`](crate::latexlike::Latexlike) is one member of a
language *family*, and a language with its own vocabularies or ext types
joins the family instead of forking the preset. What joining requires: a
`Lang` whose vocabulary and payload types implement the role traits
([`LatexlikeGroupType`](crate::latexlike::LatexlikeGroupType),
[`LatexlikeCallableType`](crate::latexlike::LatexlikeCallableType),
[`LatexlikeMode`](crate::latexlike::LatexlikeMode),
[`LatexlikeEvent`](crate::latexlike::LatexlikeEvent),
[`LatexlikeInvocationSyntax`](crate::latexlike::LatexlikeInvocationSyntax)),
plus the explicit one-line opt-in — `impl LatexlikeLang for MyLang {}`
([`LatexlikeLang`](crate::latexlike::LatexlikeLang)); the umbrella's
defaulted methods (the math-delimiter table, the math-interior forbidden
characters, the parse-initialization checks) are overridable per member.
[`Latexlike`](crate::latexlike::Latexlike) itself is the worked example of
both halves: its `Lang` implementation supplies the preset vocabularies,
the canonical seed
([`default_token_rules`](crate::latexlike::default_token_rules) plus
[`builtin_package`](crate::latexlike::builtin_package)), and the
scope-stack specials scan; and it opts into its own family exactly the way
a foreign member would.

The reuse route for behavior is the preset's **pillar functions**: the
[`LatexlikeDriver`](crate::latexlike::LatexlikeDriver)'s whole behavior is
published as family-generic building blocks —
[`math_group_interior_delta`](crate::latexlike::math_group_interior_delta),
[`exit_math_context_delta`](crate::latexlike::exit_math_context_delta),
[`make_paragraph_break_node`](crate::latexlike::make_paragraph_break_node)
— with the driver as the canned assembly whose hook bodies are one-line
delegations to them. A struct cannot be partially overridden, so a family
member wanting preset-behavior-plus-one-custom-hook writes its own
[`ParseDriver`](crate::core::ParseDriver) composing the same pillars; the
driver's documentation states that it contains no behavior the pillars do
not.

## Reusing the preset wholesale: the projection pattern

The family design carries further than variant syntaxes. The preset's
driver and spec types are generic over the family —
[`LatexlikeDriver<LLL>`](crate::latexlike::LatexlikeDriver),
[`MacroSpec<LLL>`](crate::latexlike::MacroSpec),
[`EnvironmentSpec<LLL>`](crate::latexlike::EnvironmentSpec),
[`SpecialsSpec<LLL>`](crate::latexlike::SpecialsSpec) — so a framework
language built on techy (a semantic markup language that projects parsed
documents into its own content model, say) can keep its own vocabularies,
node extensions, and invocation-syntax payload while reusing the preset's
driver behavior and declarative spec types unchanged: implement `Lang`
with your types, play the roles, opt into
[`LatexlikeLang`](crate::latexlike::LatexlikeLang), and instantiate the
preset components over your language. Forking the preset is the
alternative the family exists to avoid.

Read next: [Integration: tooling, embedding, and bindings](crate::guide::integration)
— the facts that matter when techy runs inside a larger system.
