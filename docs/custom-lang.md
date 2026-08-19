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
pieces have been introduced. This chapter builds on [The parsing model](crate::guide::parsing_model)
(what drivers, specs, and construct parsers do at parse time).

## Vocabularies: group classes and callable types

Two of `Lang`'s associated types name the language's syntactic taxonomy,
and both are **closed per language** — known when the `Lang` is written,
typically small enums:

- [`GroupTypeId`](crate::core::Lang::GroupTypeId) classifies groups (the
  preset: content vs. math vs. verbatim), fully detached from delimiter
  spellings. Which delimiter *pairs* exist, and which class each maps to,
  is runtime data — [`GroupRule`](crate::core::token::GroupRule) values in the
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
the preset never invents a "text mode" — leaving math restores an actual
enclosing context.

## Token rules and specials recognition

Tokenization is data, not code: [`TokenRules`](crate::core::token::TokenRules) —
stored in the parsing state, so all of it can change mid-parse — declares
the whitespace set, group delimiter pairs, command escape rules, comment
markers, and forbidden characters, one block per feature, each block
except forbidden characters carrying its own `enabled` flag (an empty
forbidden-character set is already its off). The type's documentation records the
detection priority and the three distinct spellings of "off": flag `false`
= disabled, data preserved; empty data = nothing recognized; and, beyond
both runtime spellings, a feature can be *absent* — the language does not
have it at all, a compile-time declaration covered in
[the next section](#declaring-which-features-the-language-has). The preset's
canonical rules
([`default_token_rules`](crate::latexlike::default_token_rules)) are the
worked example of rules as data.

A language whose tokenization *behavior* — not just data — differs
implements the [`TokenReader`](crate::core::token::TokenReader) trait instead. It
declares that reader in one place: a zero-sized type implementing
[`Tokenization`](crate::core::token::Tokenization), named as
[`Lang::Tokenization`](crate::core::Lang::Tokenization). That type states the
token type the reader produces
([`Tokenization::Token`](crate::core::token::Tokenization::Token), spelled
[`Token<L>`](crate::core::token::Token) elsewhere), the type naming a place in the
stream
([`Tokenization::StreamPosition`](crate::core::token::Tokenization::StreamPosition),
spelled [`StreamPosition<L>`](crate::core::token::StreamPosition)), and how the
reader for one parse is built; construct parsers read neither type directly,
they ask the reader. How to write one, and why the implementation needs the
bound `L: Lang<Tokenization = MyTokenization>`, is
[Implementing this trait](crate::core::token::Tokenization#implementing-this-trait),
with a compiling example. A driver may still swap the reader per instance,
through [`make_token_reader`](crate::core::ParseDriver::make_token_reader)
(see [The driver](#the-driver)). Keeping the standard token type
([`StdToken`](crate::core::token::StdToken)) is the least work: hold an
inner [`StdTokenReader`](crate::core::token::StdTokenReader) over the same content,
build tokens with the `StdToken` constructors, and delegate every question
about a token to the inner reader — the `TokenReader` page shows that shape
as a compiling example. A reader that declares a token type of its own
which wraps standard tokens — read from one source or from several, as a
macro expander does — keeps one inner `StdTokenReader` per source and works
through two of its methods:
[`scan_std_token_at`](crate::core::token::StdTokenReader::scan_std_token_at)
reads the standard token at a byte offset without moving that reader, and
[`token_kind_of_std_token`](crate::core::token::StdTokenReader::token_kind_of_std_token)
answers what one of the standard tokens it stores is; what the stream
positions on the two sides of a source change mean is
[*Seams*](crate::core::token::TokenReader#seams--readers-that-serve-several-sources-at-one-nesting-level)
on the same page. A reader with token kinds of its own builds them instead
from the *scan helpers* — free functions that each recognize one construct
at a position and return its byte spans — one helper per construct, listed
under [Writing a token reader](crate::core::token#writing-a-token-reader).

**Specials** — callables triggered by plain character sequences (`~`,
`--`) — are recognized by two `Lang` hooks, and here sits a documented
silent trap: **the two hooks must be wired together**.
[`scan_specials`](crate::core::Lang::scan_specials) answers "is a trigger
at this position?" (recognition and resolution in one call), but it is
*only consulted* when the current character is in the set returned by
[`specials_trigger_chars`](crate::core::Lang::specials_trigger_chars) —
computed once per frozen state, as a fast pre-filter. A first character
missing from that set means the trigger **silently never fires**: no error,
no diagnostic. The scan hook's documentation adds the second quiet
obligation: specials have the *lowest* recognition precedence, so a trigger
overlapping a group delimiter, escape character, or comment start also
silently never fires — the `Lang` author is the one who can create, and
must avoid, such a collision. The preset wires both hooks to the scope
stack (providers advertise their triggers), so registered specials come
with their trigger characters automatically.

## Declaring which features the language has

Token rules carry each tokenization feature's *data*; one level up, every
language also declares **at compile time which parsing features it has at
all**. [`Lang::Features`](crate::core::Lang::Features) names one
[`LangFeatures`](crate::core::LangFeatures) bundle: eight members, one
presence answer per feature — whitespace handling, paragraph breaks, group
delimiters, command syntax, comment syntax, the specials scan, and
forbidden characters (one member per
[`TokenRules`](crate::core::token::TokenRules) feature block), plus the definition
[scope stack](crate::guide::concepts_overview#scopes-and-packages). Each
member is one of exactly two marker types,
[`FeaturePresent`](crate::core::FeaturePresent) or
[`FeatureAbsent`](crate::core::FeatureAbsent) — there is no third answer.
Two ready-made bundles cover the ends:
[`AllLangFeatures`](crate::core::AllLangFeatures) declares every feature
present (what [`TrivialLang`](crate::core::TrivialLang)'s blanket
implementation supplies, and what the whole latexlike family uses), and
[`NoLangFeatures`](crate::core::NoLangFeatures) declares every feature
absent. Any other combination is a declaration you write yourself —
`LangFeatures` is deliberately open to your implementations: a unit struct
with the eight members filled in, as in the example below.

The declaration adds a third, compile-time way for a feature to be "off".
The three ways have three distinct words, never interchanged:

- **absent** — compile time: the language *has no such feature at all*. The
  member is declared [`FeatureAbsent`](crate::core::FeatureAbsent), and no
  runtime data can say otherwise.
- **disabled** — runtime, recorded in the parsing state: the feature
  block's `enabled` flag is `false` while its rules data stays in place, so
  a later state delta can re-enable the feature losslessly.
- **empty** — a property of the rules data itself: the data holds nothing
  (no group delimiter pairs, an empty whitespace set), so nothing is
  recognized even with the `enabled` flag `true`.

At parse time, absence means the feature's syntax is simply not recognized:
its characters read as plain content (to a language without the commands
feature, `\emph` is five ordinary characters). Absence also goes all the
way to storage. An absent feature's field — in
[`TokenRules`](crate::core::token::TokenRules), and equally in the override type
[`TokenRulesOverrides`](crate::core::token::TokenRulesOverrides) — holds a
zero-sized placeholder instead of the feature's rules block: a value that
carries no data and takes no space, so a language declaring every feature
absent stores literally nothing for its rules. And because the field's type
is the placeholder, rules data for an absent feature *cannot be written*: a
rules or override literal for that field is a compile error at the site
that writes it, not a runtime report.

For a *present* feature the opposite holds, and it is a guarantee rather
than an optimization: the stored type **is** the plain rules type, with no
wrapper around it. A language with every feature present writes plain
struct literals and plain field reads exactly as if this mechanism did not
exist — if your language uses the full syntax, declare
[`AllLangFeatures`](crate::core::AllLangFeatures) and you are done with
this section.

A partial language builds its seed rules by writing the present features'
blocks as plain literals and taking every other field from
[`TokenRules::empty()`](crate::core::token::TokenRules::empty) with struct-update
syntax. `empty()` answers for every language — a present feature's field
gets its block's all-empty value, an absent feature's field the zero-sized
placeholder — so the spread fills exactly the fields a literal could not
name. A complete braces-only language:

```rust
# use std::sync::Arc;
# use techy::core::node::{NodeKind, StagedChildren};
# use techy::core::specs::ScopeStack;
# use techy::error::Recovery;
# use techy::source::SourceSpan;
use techy::core::token::{GroupRule, GroupRules, StdTokenization, TokenRules};
use techy::core::{
    FeatureAbsent, FeaturePresent, FinalizeError, Lang, LangFeatures, Language,
    ParsingState, StateData, StdParseDriver,
};

// The declaration: group delimiters present, the seven other features absent.
struct BracesOnlyFeatures;

impl LangFeatures for BracesOnlyFeatures {
    type Whitespace = FeatureAbsent;
    type Paragraphs = FeatureAbsent;
    type Groups = FeaturePresent;
    type Commands = FeatureAbsent;
    type Comments = FeatureAbsent;
    type Specials = FeatureAbsent;
    type ForbiddenChars = FeatureAbsent;
    type Scopes = FeatureAbsent;
}

#[derive(Debug, Clone, Copy)]
struct BracesOnlyLang;

impl Lang for BracesOnlyLang {
    type Features = BracesOnlyFeatures;
    // ... the remaining associated types as usual (`u32` ids, `()` exts) ...
#     type GroupTypeId = u32;
#     type CallableTypeId = u32;
#     type ModeId = ();
#     type StateExt = ();
#     type Event = ();
#     type SessionExt = ();
#     type SourceOrigin = Option<String>;
#     type Tokenization = StdTokenization;
#     type NodeExts = ();
#     type InvocationSyntax = ();
#     type Driver = StdParseDriver;

    fn initial_state_data() -> Result<StateData<Self>, FinalizeError> {
        Ok(StateData {
            rules: TokenRules {
                // The present feature's block: a plain struct literal.
                groups: GroupRules {
                    enabled: true,
                    rules: vec![Arc::new(GroupRule {
                        group_type: 0,
                        open: "{".into(),
                        close: "}".into(),
                    })],
                    temporary: vec![],
                    expecting_close: None,
                },
                // Every other field: spread from `empty()`.
                ..TokenRules::empty()
            },
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        })
    }
#
#     fn make_node_ext(
#         _kind: &NodeKind<Self>,
#         _span: &SourceSpan<Option<String>>,
#         _state: &Arc<ParsingState<Self>>,
#         _children: StagedChildren<'_, Self>,
#     ) -> Result<(), techy::core::node::NodeBuildError> {
#         Ok(())
#     }
}

// Braces parse as a group; the command escape, the comment marker, and the
// spaces are plain content — those features do not exist here.
let language: Language<BracesOnlyLang> = Language::new(
    StdParseDriver::new(Recovery::Strict, ()),
    ParsingState::lang_initial().expect("seed state"),
);
let result = language.parse(r"a{b} \c %d").unwrap();
let root = result.tree.root();
assert!(root.child(1).unwrap().is_group());
assert_eq!(root.child(2).unwrap().chars(), Some(r" \c %d"));
```

Code that only makes sense with a feature present says so in its signature,
through one bound trait per feature:
[`LangHasWhitespace`](crate::core::LangHasWhitespace),
[`LangHasParagraphs`](crate::core::LangHasParagraphs),
[`LangHasGroups`](crate::core::LangHasGroups),
[`LangHasCommands`](crate::core::LangHasCommands),
[`LangHasComments`](crate::core::LangHasComments),
[`LangHasSpecials`](crate::core::LangHasSpecials),
[`LangHasForbiddenChars`](crate::core::LangHasForbiddenChars), and
[`LangHasScopes`](crate::core::LangHasScopes). You never implement these
traits: a blanket implementation covers every `Lang` whose declaration
makes the feature present, so each exists purely as a bound. The bounds a
language author actually encounters, and why each holds:

- **Verbatim parsing and the group-minting argument parsers require
  [`LangHasGroups`](crate::core::LangHasGroups).** A verbatim region's
  terminator is a group rule matched by the group-close machinery
  ([`verbatim_state_delta`](crate::core::constructs::verbatim_state_delta),
  [`VerbatimArgumentParser`](crate::core::constructs::VerbatimArgumentParser),
  [`VerbatimBodyParser`](crate::core::constructs::VerbatimBodyParser)), and
  the argument parsers that recognize per-use delimiters by minting
  temporary group rules (bracketed optional arguments, for instance) write
  group data — both are built out of the group feature.
- **Scope mutation requires
  [`LangHasScopes`](crate::core::LangHasScopes).**
  [`ScopeStack::push`](crate::core::specs::ScopeStack::push), the delta
  builders
  [`ParsingStateDelta::scope_op`](crate::core::ParsingStateDelta::scope_op)
  and
  [`ParsingStateDelta::push_provider`](crate::core::ParsingStateDelta::push_provider),
  and the package-preloading
  [`ParsingState::lang_initial_with_packages`](crate::core::ParsingState::lang_initial_with_packages)
  all add definitions to the scope stack — which only a language with the
  scopes feature has.
- **[`LangHasParagraphs`](crate::core::LangHasParagraphs) itself requires
  [`LangHasWhitespace`](crate::core::LangHasWhitespace).** A paragraph
  break is a whitespace run containing two or more newlines, detected
  inside the reader's whitespace handling, so a language cannot have
  paragraph breaks without whitespace handling; the requirement is checked
  by the compiler.

One independence is equally deliberate: **commands do not imply scopes**. A
language whose command set is fixed resolves every command from a table on
its driver (its [`CommandResolver`](crate::core::CommandResolver)) and can
declare the scope stack absent.

The declarations also shape the two whole-value override constructors.
[`TokenRulesOverrides::disable_all()`](crate::core::token::TokenRulesOverrides::disable_all)
disables every feature the language *has*: it consults the presence
declarations and flips the `enabled` flag of exactly the present features
(forbidden characters, which have no flag, are never touched) — absent
features are simply not mentioned by the value it returns — so it
can never fail, whatever the language declares. Its counterpart
[`TokenRulesOverrides::override_all()`](crate::core::token::TokenRulesOverrides::override_all)
reads the same declarations the other way round: it copies the given
[`TokenRules`](crate::core::token::TokenRules) into overrides for exactly the
present features, so applying it installs those rules wholesale (the two
transient group fields excepted — see its documentation). Finally, one short note
that matters mainly to custom tooling: the frozen state's two derived
lookup caches follow the declarations —
[`ParsingState::prefix_table()`](crate::core::ParsingState::prefix_table)
and
[`ParsingState::trigger_chars()`](crate::core::ParsingState::trigger_chars)
return `Option`, `None` exactly when the corresponding feature (groups,
specials) is absent, while a present-but-disabled feature still answers
`Some` of the frozen empty value.

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
- **The node ext hook**:
  [`make_node_ext`](crate::core::Lang::make_node_ext) is the language's
  one chance to compute per-node data, with the node's parts in view — and
  it is the **only required `Lang` method** (a no-ext language writes the
  `Ok(())` one-liner). Its page documents exactly what the hook can see (the
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
  post-space, an environment's begin/end syntax) in the language's own form. As its
  documentation puts it, this channel is what makes *recomposition
  accuracy the language's choice* — byte-exact re-emission is possible
  exactly to the extent the language records spelling facts here and obeys
  span tiling
  ([`OBEYS_SPAN_TILING`](crate::core::Lang::OBEYS_SPAN_TILING)); with
  `OBEYS_SPAN_TILING = false` the recomposer re-emits the tree as stored,
  claiming no byte-equality with any one source. `()`
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
unique derivation (what it accumulates into the session extension is handed
out on [`ParseResult::session_ext`](crate::core::ParseResult::session_ext)). Semantic [events](crate::core::Lang::Event) come in the
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
persistence enabled) — the caller applies it in a *single* derivation: the
**forwarding construct's own transition** carries the merged delta, not one
call per original after-effect, so an intermediate value of the
after-effect chain never appears in that transition. This collapse concerns
sibling after-effects only, never descents: inside the included run nothing
changes — each after-effect there is its own ordinary transition, and a
group descent inside the included file (a math group, say) is an ordinary
child-state derivation that reaches `finalize_transition` like any other.
Worked example — `\one` and `\two` each carry a
[`MacroSpec::with_after_effect`](crate::latexlike::MacroSpec::with_after_effect)
delta overriding the same state-extension field:

```text
main.tex: a \input{defs.tex} b          defs.tex: \one \two
inside the included run:  two transitions — the field goes (unset → "one"), ("one" → "two")
the forwarding transition: one          — the field goes (unset → "two"); "one" never appears
```

Design customizers against the merged form — the
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
the group descent-delta channel, and construct provision. Every method has a
working default — `impl ParseDriver<MyLang> for MyDriver {}` is already a
complete driver — so override only what your language needs. Tokenization is
defaulted too:
[`make_token_reader`](crate::core::ParseDriver::make_token_reader) builds the
reader the language's own
[`Tokenization`](crate::core::Lang::Tokenization) names ([Token rules and
specials recognition](#token-rules-and-specials-recognition) above). Override
that hook when the reader needs data the driver *instance* holds; a reader
needing none belongs on the language. (Parsing depth is
limited by the engine's own guard, configured on the language value with
[`with_descent_guard_init`](crate::core::Language::with_descent_guard_init)
— it is not a driver concern.)

**Command resolution** is the hook a command-bearing language cannot leave
defaulted — the core cannot know which of your callable types commands
resolve under. The ready-made driver
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
neutral (no modes, no exts, a driver that resolves nothing; every feature
is declared present but the seed state's rules are empty, so content is
plain characters until you derive rules into it):

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
([`LatexlikeLang`](crate::latexlike::LatexlikeLang)); the trait's
defaulted methods (the math-delimiter table, the math-interior forbidden
characters, the parse-initialization checks) are overridable per member.
[`Latexlike`](crate::latexlike::Latexlike) itself is the worked example of
both halves: its `Lang` implementation supplies the preset vocabularies,
the canonical seed
([`default_token_rules`](crate::latexlike::default_token_rules) plus
[`builtin_package`](crate::latexlike::builtin_package)), and the
scope-stack specials scan; and it opts into its own family exactly the way
a foreign member would.

The reuse route for behavior is the preset's **behavior functions**: the
[`LatexlikeDriver`](crate::latexlike::LatexlikeDriver)'s whole behavior is
published as family-generic free functions —
[`math_group_interior_delta`](crate::latexlike::math_group_interior_delta),
[`exit_math_context_delta`](crate::latexlike::exit_math_context_delta),
[`make_paragraph_break_node`](crate::latexlike::make_paragraph_break_node)
— with the driver as the ready-made assembly whose hook bodies are one-line
delegations to them. A struct cannot be partially overridden, so a family
member wanting preset-behavior-plus-one-custom-hook writes its own
[`ParseDriver`](crate::core::ParseDriver) composing the same functions; the
driver's documentation states that it contains no behavior these functions do
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
with your types, implement the role traits, opt into
[`LatexlikeLang`](crate::latexlike::LatexlikeLang), and instantiate the
preset components over your language. Forking the preset is the
alternative the family exists to avoid.

The environment pair's own spelling is data too, and needs no custom language
at all: the opening command is named by the entry it is registered under, and
the terminator by the argument to
[`BeginSpec::new`](crate::latexlike::BeginSpec::new). A language writing its
environments `\open{name} … \shut{name}` registers
`BeginSpec::new("shut")` under `"open"` and an
[`EndSpec`](crate::latexlike::EndSpec) under `"shut"`, in a package of its
own — everything downstream, including verbatim bodies and the re-emitted
source, follows those names.

Read next: back to the [Developer Guide](crate::guide#developer-guide) index —
the other chapters on extending and embedding techy.
