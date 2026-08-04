# Concepts overview

The main concepts of techy, in one place. Each section is a compact,
self-contained explanation pointing to the API items that embody the concept.
Other documentation pages link into this page; the section headings are stable
anchors and keep their names.

*(This page starts as a skeleton: several sections are brief placeholders to be
expanded.)*

## Sources and spans

A [`Source`](crate::source::Source) owns the content being parsed and is shared
as `Arc<Source>`. A [`Span`](crate::source::Span) is a plain byte range into
that content; a [`SourceSpan`](crate::source::SourceSpan) pairs the range with
its source so it remains meaningful after the parse. Line/column positions are
a lazy display concern ([`LineIndex`](crate::source::LineIndex); the
consumer-held [`LineIndexCache`](crate::source::LineIndexCache) is its
persistent, per-source form), never parsing inputs. Content lookup for
`\input`-like constructs is delegated to the embedder through
[`SourceResolver`](crate::source::SourceResolver); resolved content parses into
the same tree at the inclusion point, so trees spanning several sources are
first-class.

## Tokens and token rules

[`Token`](crate::token::Token)s are minimal, structural, zero-copy views of the
source ([`TokenKind`](crate::token::TokenKind) is a small closed set), produced
by a [`TokenReader`](crate::token::TokenReader). Tokenization behavior is plain
data — [`TokenRules`](crate::token::TokenRules) stored in the parsing state —
so it can change mid-parse through state transitions.

## Parsing state and deltas

A [`ParsingState`](crate::core::ParsingState) is an immutable snapshot of
everything that can vary during a parse. Changes are expressed as reified
[`ParsingStateDelta`](crate::core::ParsingStateDelta) values, applied at a
single transition point to derive a new state.

## The `Lang` generic

The [`Lang`](crate::core::Lang) trait bundles all compile-time customization:
extension types, hooks, and the parse driver. Every core type takes the single
`L: Lang` parameter.

## Modes

The parsing mode is first-class state data: a per-language `Lang::ModeId` names
the mode a parsing state is in (the latexlike preset uses text and math modes).
Deltas initiate mode changes; the language interprets them at the transition
point.

## Callable specs and arguments

Anything invocable — macros, environments, specials in LaTeX terms — resolves
to a [`CallableSpec`](crate::core::specs::CallableSpec), which describes the
invocation's argument structure ([`ArgumentSpec`](crate::core::specs::ArgumentSpec),
[`ArgumentParser`](crate::core::constructs::ArgumentParser)) and supplies the parser for
its invocations.

## Scopes and packages

Definitions live in [`Package`](crate::core::specs::Package)s; a parse resolves
names through a [`ScopeStack`](crate::core::specs::ScopeStack) of
[`Scope`](crate::core::specs::Scope)s with lexical shadowing. Lookup is served by
the [`SpecsProvider`](crate::core::specs::SpecsProvider) contract.

## The node tree

A parse produces a flat, frozen [`NodeTree`](crate::core::node::NodeTree). Structure
is the closed [`NodeKind`](crate::core::node::NodeKind) (characters, group, callable
invocation, comment, list); custom per-node data attaches through extension
types supplied by `Lang`. Nodes are read through
[`NodeRef`](crate::core::node::NodeRef) proxies; the structured traversal is
[`visit::walk`](crate::visit::walk), tree→tree transformation is
[`transform::restage`](crate::transform::restage), and tree→text (or any
composed value) is [`recompose::recompose`](crate::recompose::recompose) — the
preset's [`source_recomposer`](crate::latexlike::source_recomposer) reemits a
tree's source spelling from its recorded facts.

## Construct parsers

Each syntactic construct is parsed by a
[`ConstructParser`](crate::core::constructs::ConstructParser); the content dispatch
loop ([`NodesParser`](crate::core::constructs::NodesParser)) selects parsers by token
kind and definition lookup, with everything a parser needs carried in one
[`ParseContext`](crate::core::constructs::ParseContext).

## The engine

A [`Language`](crate::core::Language) bundles a ready-to-use language for
embedders; parsing runs in a [`ParserSession`](crate::core::ParserSession)
and yields a [`ParseResult`](crate::core::ParseResult). Parse-driving
behavior — recovery policy, parse-time hooks — lives on the language's
[`ParseDriver`](crate::core::ParseDriver).

## Diagnostics and tolerant parsing

Problems surface as structured conditions
([`Diagnostic`](crate::error::Diagnostic), collected in
[`Diagnostics`](crate::error::Diagnostics)), not prose. Tolerant parsing
recovers where a problem is detected and still yields a best-effort tree; an
`Err` from the machinery means abort.

## The latexlike preset

The familiar LaTeX behavior — text/math modes, `\begin`/`\end` environments,
default token rules — packaged as the
[`Latexlike`](crate::latexlike::Latexlike) language, implemented entirely
through the public extension points described above.
