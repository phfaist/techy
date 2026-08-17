# Concepts overview

The main concepts of techy, in one place. Each section is a compact,
self-contained explanation pointing to the API items that embody the concept.
Other documentation pages link into this page; the section headings are stable
anchors and keep their names.

## Sources and spans

A [`Source`](crate::source::Source) owns one unit of source content together
with its origin metadata, and is shared as `Arc<Source>`. A
[`Span`](crate::source::Span) is a plain `Copy` byte range into content — the
transient type on which all span arithmetic rests — while a
[`SourceSpan`](crate::source::SourceSpan) pairs a byte range with its
`Arc<Source>`, so nodes and diagnostics that carry one are self-contained and
remain meaningful after the parse (no lifetime parameters, no external source
store). [`SourcePos`](crate::source::SourcePos) is the single-position
counterpart, used to query parsed trees by location. Every source records its
[`SourceProvenance`](crate::source::SourceProvenance) — primary input, resolved
content, or synthesized — with a back-reference to the span that triggered it,
forming a provenance chain walkable for error reporting. Line/column positions
are a lazy display concern ([`LineIndex`](crate::source::LineIndex); the
consumer-held [`LineIndexCache`](crate::source::LineIndexCache) is its
persistent, per-source form) — parsing itself works purely in byte offsets.
Content lookup for `\input`-like constructs is delegated to the embedder
through [`SourceResolver`](crate::source::SourceResolver); resolved content
parses into the same tree at the inclusion point, so trees spanning several
sources are first-class. Recursion and cycle policy stays with the embedder —
[`Source::including_sources`](crate::source::Source::including_sources) and
[`check_include_chain`](crate::source::check_include_chain) are the ready-made
policy tools.

## Tokens and token rules

[`Token`](crate::core::Token)s are minimal, structural, opaque values produced
by a [`TokenReader`](crate::core::TokenReader). Which token type a language
uses is its own declaration ([`Lang::Token`](crate::core::Lang::Token)); the
standard reader ([`StdTokenReader`](crate::core::StdTokenReader)) produces
[`StdToken`](crate::core::StdToken)s. Nothing is read off a token directly: a
parser asks the reader what a token *is*
([`token_kind`](crate::core::TokenReader::token_kind)) and where it is
([`source_span_of`](crate::core::TokenReader::source_span_of) and its
companions, which answer with a [span](#sources-and-spans) — a source and a
byte range in it — for the whole token or for the stretch between two of its
[edges](crate::core::TokenEdge), the five boundaries running from where its
leading whitespace begins to where its trailing whitespace ends). A place in
the token *stream*, as opposed to a place in the text, is a
[`Lang::StreamPosition`](crate::core::Lang::StreamPosition): opaque as well,
handed out by the reader alone
([`position_here`](crate::core::TokenReader::position_here),
[`position_at`](crate::core::TokenReader::position_at)), and the value a
parser uses to send the stream back to a place it has been
([`move_to_position`](crate::core::TokenReader::move_to_position)) or to ask
for the span between two such places. A token is an atomic unit identifying
*what to parse next*:
[`TokenKind`](crate::core::TokenKind) — the reader's answer — is a small closed
set, a
[`Char`](crate::core::TokenKind::Char) token covers exactly one character
(character runs accumulate into nodes at the node level, not in the reader),
and a terminal [`EndOfStream`](crate::core::TokenKind::EndOfStream) token ends
every stream. Tokens carry no macro/environment taxonomy: `\begin` is a
[`Command`](crate::core::TokenKind::Command) token like any other, and what its
name means is decided at parse time — the one exception is
[`Specials`](crate::core::TokenKind::Specials), where recognition *is*
resolution, so the reader's answer for the token already names its
[spec](#callable-specs-and-arguments). Tokenization behavior is plain data
— [`TokenRules`](crate::core::TokenRules) stored in the parsing state — so it
can change mid-parse through state transitions.

## Parsing state and deltas

A [`ParsingState`](crate::core::ParsingState) is an immutable snapshot of
everything that can vary during a parse: the stored settings
([`StateData`](crate::core::StateData) — token rules, the parsing mode, the
language's own state extension), the definitions currently visible (the
[scope stack](#scopes-and-packages)), and derived lookup caches. Changes are
expressed as reified [`ParsingStateDelta`](crate::core::ParsingStateDelta)
values — typed overrides plus semantic events, data rather than closures, so
deltas are mergeable, inspectable, and applicable by a *caller* to a base
state the producer never saw.
[`ParsingState::derived`](crate::core::ParsingState::derived) is the sole
constructor of non-initial states: it applies the delta, runs the language's
[`finalize_transition`](crate::core::Lang::finalize_transition) customizer
exactly once, and freezes the result; the seed state comes from
[`ParsingState::lang_initial`](crate::core::ParsingState::lang_initial).
Cross-cutting rules ("in math mode the escape character changes") live in the
customizer, not in every delta writer.

## The `Lang` generic

The [`Lang`](crate::core::Lang) trait bundles all compile-time customization
of the machinery, and every core type takes the single `L: Lang` parameter. A
language chooses its vocabulary and extension types (mode identifiers,
callable and group types, node extension bundle, state extension), supplies
the canonical initial state data, and implements the language hooks — the
state-transition customizer
([`finalize_transition`](crate::core::Lang::finalize_transition)) and the
specials-recognition hooks of the token layer.
[`TrivialLang`](crate::core::TrivialLang) is the all-defaults language for
tests and experiments with the machinery.

## Modes

The parsing mode is first-class state data: a per-language `Lang::ModeId`
names the mode a parsing state is in (the latexlike preset uses text and math
modes). Deltas initiate mode changes; the language interprets them at the
transition point, where mode-dependent adjustments to the rest of the state
belong to the [`finalize_transition`](crate::core::Lang::finalize_transition)
customizer.

## Callable specs and arguments

A *callable* is anything invocable from the token stream — macros,
environments, and specials, in LaTeX terms. Its behavior is recorded by a
[`CallableSpec`](crate::core::specs::CallableSpec), which is not tied to any
particular name: one spec may back several names. The declarative surface is
the list of [`ArgumentSpec`](crate::core::specs::ArgumentSpec)s describing the
invocation's arguments; the behavioral surface is a factory returning the
[construct parser](#construct-parsers) for each resolved invocation —
overriding the default parser is the full-takeover route for `\verb`-like
constructs. [`StdCallableSpec`](crate::core::specs::StdCallableSpec) is the
standard declarative implementation, and the
[`ArgumentParser`](crate::core::constructs::ArgumentParser) contract (with the
shipped argument-parser implementations) lives in
[`core::constructs`](crate::core::constructs). Specs are `Send + Sync` by
contract: they are stored in parsed trees.

## Scopes and packages

Definitions are served by [`SpecsProvider`](crate::core::specs::SpecsProvider)s
arranged in a [`ScopeStack`](crate::core::specs::ScopeStack), searched
innermost-first (lexical shadowing).
[`Package`](crate::core::specs::Package) is the immutable provider (a
distributable set of definitions); [`Scope`](crate::core::specs::Scope) is the
mutable-by-replacement provider; a
[`FallbackProvider`](crate::core::specs::FallbackProvider) expresses the
unknown-callable policy. Definitions change mid-parse through scope operations
carried by [parsing state deltas](#parsing-state-and-deltas) (`\newcommand`,
package loads), and scopes revert structurally when the enclosing group ends
and parsing resumes with the outer state, which still holds the previous
scope stack. Name lookup during a parse is the command-resolution family around
[`resolve_command_in_scopes`](crate::core::specs::resolve_command_in_scopes).

## The node tree

A parse produces a flat, frozen [`NodeTree`](crate::core::node::NodeTree):
all nodes in one indexed store, only read after the parse. Structure is the
closed [`NodeKind`](crate::core::node::NodeKind) — characters, group, callable
invocation, comment, list; custom per-node data attaches through extension
types supplied by `Lang`. Nodes are read through
[`NodeRef`](crate::core::node::NodeRef) proxies and
[`NodeSlice`](crate::core::node::NodeSlice) views; payloads record the parsed
facts ([`GroupData`](crate::core::node::GroupData),
[`CallableData`](crate::core::node::CallableData) with its parsed arguments
and content regions). New trees are assembled through a
[`NodeTreeBuilder`](crate::core::node::NodeTreeBuilder). The consumer toolkit
is structured per module: extraction helpers in [`extract`](crate::extract),
the structured traversal [`visit::TreeWalker`](crate::visit::TreeWalker), tree→tree
transformation [`transform::TreeRestager`](crate::transform::TreeRestager), and
tree→value recomposition [`recompose::TreeRecomposer`](crate::recompose::TreeRecomposer)
— the preset's
[`source_recomposer`](crate::latexlike::source_recomposer) reemits a tree's
source spelling from its recorded facts.

## Construct parsers

Each syntactic construct is parsed by a
[`ConstructParser`](crate::core::constructs::ConstructParser) implementation —
the content dispatch loop
([`NodesParser`](crate::core::constructs::NodesParser)) selects parsers by
token kind and definition lookup — reading tokens and staging nodes through
one [`ParseContext`](crate::core::constructs::ParseContext). Construct parsers
are temporaries: constructed with their per-use configuration, carrying
working state in fields, dropped when their parse ends — while stored behavior
objects (specs, argument parsers) are `Arc`-shared and immutable. A parser
returns its output together with an optional state delta that is exclusively
the construct's *after-effect for the caller* (as with `\newcommand`); an
`Err` from a construct parser means abort, and recovery happens at the
detection site.

## The engine

A [`Language`](crate::core::Language) bundles a ready-to-use language for
embedders — seed state, [`ParseDriver`](crate::core::ParseDriver) instance,
and optional source resolver — and its
[`parse()`](crate::core::Language::parse) entry point runs a whole parse.
Parsing accumulates into a [`ParserSession`](crate::core::ParserSession) (the
staged tree, the diagnostics sink, the live frame stack), which is frozen into
a [`ParseResult`](crate::core::ParseResult); sessions are transient — one
parse each. A `ParseResult` holds no reference to the `Language`, so results
outlive their bundle. Parse-driving *behavior* — recovery policy, parse-time
hooks, command resolution strategy — lives on the driver, not the session.

## Diagnostics and tolerant parsing

Problems surface as structured conditions, not prose: a
[`Diagnostic`](crate::error::Diagnostic) (collected in
[`Diagnostics`](crate::error::Diagnostics), available on the parse result)
carries a typed condition payload plus span and traceback frames; the human
message is derived from the payload, and machine consumers match the concrete
condition type or its stable identifier string. Third-party condition types
are structurally identical to the library's own — implement
[`DiagnosticInfo`](crate::error::DiagnosticInfo) on a data struct and it flows
through the same carriers. The strict/tolerant decision is the
[`Recovery`](crate::error::Recovery) policy: tolerant parsing recovers where a
problem is detected and still yields a best-effort tree, while an `Err` from
the machinery ([`ParseError`](crate::error::ParseError)) means abort.

## The latexlike preset

The familiar LaTeX behavior — text/math modes, `{`…`}` groups, `%` comments,
`\` commands, `\begin`/`\end` environments, verbatim, default token rules —
packaged as the [`Latexlike`](crate::latexlike::Latexlike) language,
implemented entirely as preset data and preset code over the same extension
surface any language uses. The preset supplies the
[`LatexlikeDriver`](crate::latexlike::LatexlikeDriver) (recovery policy and
scope-stack command resolution), the seed data
([`default_token_rules`](crate::latexlike::default_token_rules),
[`builtin_package`](crate::latexlike::builtin_package)), the declarative spec
types ([`MacroSpec`](crate::latexlike::MacroSpec),
[`EnvironmentSpec`](crate::latexlike::EnvironmentSpec),
[`SpecialsSpec`](crate::latexlike::SpecialsSpec)), and the opt-in
[`minidefs`](crate::latexlike::minidefs) prototyping package (never
preloaded). A framework language with its own vocabularies joins the language
family (the [`LatexlikeLang`](crate::latexlike::LatexlikeLang) family trait)
instead of forking the preset.

Read next: back to the [Developer Guide](crate::guide#developer-guide) index —
the other chapters on extending and embedding techy.
