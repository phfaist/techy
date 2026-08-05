# Language syntax

This chapter describes what a **latexlike language** looks like on the page:
the syntactic constructs that techy's [`latexlike`](crate::latexlike) preset
recognizes — plain text, groups, comments, macros, environments, and specials
— and where their meaning comes from. It ends with the one fact worth keeping
in mind throughout: this syntax is a *preset*, not the engine — the parsing
engine itself has no built-in language concepts, and the familiar LaTeX
behavior described here is data and code layered on top of it.

A latexlike language is any markup language built from these constructs. LaTeX
is the obvious member, but the preset is deliberately more general: the escape
character, the group delimiters, the comment markers, and every definition are
configuration, so a language that merely resembles LaTeX — a documentation
markup, a template language, a math-formula dialect — is defined with the same
pieces rather than approximated by a LaTeX parser.

Parsing source text produces a [node
tree](crate::guide::concepts_overview#the-node-tree), a structured
representation your program reads and transforms; the
[Node trees](crate::guide::node_trees) chapter covers it. Here we stay on the
source side: what the text means to the parser.

## Plain text, whitespace, and paragraph breaks

Anything not claimed by another construct is plain character content, and
consecutive plain characters accumulate into a single character-run node.
Whitespace is part of that content. The default rules treat the ASCII
whitespace characters (space, tab, newline, carriage return, vertical tab,
form feed) as whitespace — deliberately not the full Unicode set, so that for
example a non-breaking space is ordinary content
([`default_token_rules`](crate::latexlike::default_token_rules)).

A whitespace run containing two or more newlines is a **paragraph break**. By
default it stays a whitespace-only character node; a driver setting
([`ParagraphBreakStyle`](crate::latexlike::ParagraphBreakStyle)) switches to
emitting a dedicated paragraph-break node instead.

```text
First paragraph.

Second paragraph.
```

## Groups

A **group** is a delimited region whose content is parsed as a unit. The
default rules declare `{`…`}` as the plain content group: its interior parses
exactly like the surrounding content.

```text
gather {several words} into one unit
```

Math is also a kind of group. The default rules declare four math delimiter
pairs — `$…$`, `$$…$$`, `\(…\)`, and `\[…\]` — whose interiors parse in
**math mode** rather than text mode. Which parsing mode a piece of content was
parsed in is recorded on every node, and definitions can be registered as
visible only in certain modes. The inline/display distinction (`$…$` versus
`$$…$$`) does not change how the interior parses; it is recorded on the group
as its [`MathGroupForm`](crate::latexlike::MathGroupForm), for consumers such
as renderers. At a position where `$` could either close the current inline
math group or open a display one, closing wins: `$a$$b$` is two inline math
groups ([`default_token_rules`](crate::latexlike::default_token_rules)).

```text
inline $x + y$ and display \[ x^2 + y^2 \]
```

Square brackets are deliberately *not* group delimiters: in LaTeX, `a [b] c`
is plain text. The bracketed form `[…]` is recognized only where a definition
declares an optional argument, through a temporary group rule active exactly
at that position (see
[Defining macros, environments, and specials](crate::guide::specs)).

All of these delimiter pairs are configuration, not built-ins: each is a group
rule in the language's token rules, carrying a group class that determines how
the interior is parsed ([`TokenRules`](crate::core::TokenRules),
[`GroupType`](crate::latexlike::GroupType)).

## Comments

A comment runs from a start delimiter — `%` in the default rules — to the end
of the line. The comment's text is preserved in the parse output as a comment
node, not discarded. Several comment syntaxes may coexist, and comment
recognition can be disabled entirely
([`CommentRule`](crate::core::CommentRule)).

```text
visible text  % this note runs to the end of the line
```

## Macros

A **command token** is an escape character followed by a name: `\emph`,
`\begin`, `\&`. With the default rules the escape character is `\` and names
are made of letters; a single non-letter character after the escape forms a
single-character command like `\&`
([`CommandRule`](crate::core::CommandRule)).

What a command *means* is not decided by the tokenizer. At parse time the name
is looked up among the definitions currently in scope; a resolved command
parses as a **macro** invocation — the definition (its *spec*) declares which
arguments follow, and the parser consumes them:

```text
\emph{emphasized}  \cite[Lemma 3]{knuth}  \item
```

Here `\emph`'s definition declares one mandatory `{…}` argument, `\cite`'s an
optional `[…]` argument followed by a mandatory one, and `\item`'s a single
optional argument. Argument declarations, and everything else about defining
macros, are the subject of
[Defining macros, environments, and specials](crate::guide::specs).

A command with a multi-character name also absorbs the whitespace immediately
following it (the *post-space*: `\emph x` invokes `\emph`, with the blank
recorded on the invocation, not lost) — as in TeX
([`CommandRule`](crate::core::CommandRule)).

A command whose name resolves to no definition is an error — reported with a
source span and, in tolerant parsing, recovered so the parse continues (see
[Running the parser](crate::guide::parsing)).

Here is everything so far in one parse. The
[`summary()`](crate::core::node::NodeRef::summary) helper renders a compact
one-line description of each parsed node:

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial(),
);
let result = language.parse("Hello {brave} $x+y$ world % bye").unwrap();
let shapes: Vec<String> =
    result.tree.root().children().iter().map(|node| node.summary()).collect();
assert_eq!(shapes, [
    "chars(Hello )",
    "group(Content { })",
    "chars( )",
    "group(Math(Inline) $ $)",
    "chars( world )",
    "comment( bye)",
]);
```

## Environments

An **environment** is the `\begin{name} … \end{name}` construct: a named
region with a body, optionally taking arguments after `\begin{name}`.

```text
\begin{enumerate}[(i)]
  \item First point.
  \item Second point.
\end{enumerate}
```

Environments are not a separate token-level construct: `\begin` and `\end` are
ordinary command tokens, and their definitions (shipped in the preset's
builtin package) implement the environment composition — resolving the
environment's own definition by the name in braces, parsing its declared
arguments, then parsing the body up to the matching `\end{name}`
([`builtin_package`](crate::latexlike::builtin_package)). An environment's
definition can also change how its body parses — an `equation`-like
environment switches its body to math mode, and a `verbatim`-like environment
reads its body as raw, untokenized text (see
[Defining macros, environments, and specials](crate::guide::specs)).

## Specials

A **specials** construct is a character sequence that carries meaning without
an escape character: LaTeX's non-breaking tie `~`, or the typography ligatures
`` `` ``, `''`, `--`, `---`. Like macros, specials are definitions: a
character sequence is registered as a trigger, and when the parser encounters
it, the invocation parses with the declared arguments (a specials definition
can take arguments — think of a table-alignment `&` or a subscript `_`).
Where several registered triggers could match, the longest match wins; each
definition can be restricted to certain parsing modes — the shipped ligatures,
for example, are text-only, so inside `$…$` they stay plain characters
([`minidefs`](crate::latexlike::minidefs)).

```text
pages 12--14 --- see ``the appendix''
```

Unregistered sequences are simply plain characters: with no definitions
loaded, `~` and `---` parse as ordinary text.

## Definitions can change during the parse

Which macros, environments, and specials exist is not fixed for the whole
document: definitions live in
[scopes](crate::guide::concepts_overview#scopes-and-packages) — a stack of
definition providers searched innermost-first — and the scope stack can change
*during* the parse. A definition can be added mid-document (as `\newcommand`
does in LaTeX), a package of definitions can be loaded, and a definition can
be confined to a region: an environment's body can carry its own definitions,
which stop applying past `\end`. The preset's opt-in
[`minidefs`](crate::latexlike::minidefs) package ships the exemplar: `\item`
is defined only inside `itemize`/`enumerate` bodies.

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::minidefs::minilatex_package;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([minilatex_package()]),
);

// Inside a list environment's body, `\item` is defined…
assert!(language.parse(r"\begin{itemize}\item one\end{itemize}").is_ok());

// …outside of one, the same name does not resolve.
assert!(language.parse(r"\item one").is_err());
```

Even the tokenization rules themselves — group delimiters, escape characters,
comment markers — are part of the changeable parsing state, which is how
verbatim regions and math-mode rule changes work.

## No definitions ship by default

Out of the box, the preset defines the *syntax* above but almost no
*vocabulary*: the only shipped definitions are `\begin` and `\end` (the
builtin package's environment dispatch). `\emph` is an unresolvable command
until you register it. This is deliberate: any shipped definition set would
fall short of a real LaTeX definitions database, while applications built on
techy know exactly which definitions they want
([`builtin_package`](crate::latexlike::builtin_package)).

Two ways to get definitions:

- **Register your own** — the normal path for applications. See
  [Defining macros, environments, and specials](crate::guide::specs).
- **Load the toy package** — for quick starts and debugging, the opt-in
  [`minidefs::minilatex_package()`](crate::latexlike::minidefs::minilatex_package)
  ships a handful of familiar definitions (`\emph`, `\textbf`, `\textit`, the
  list environments with `\item`, the typography specials). It is never
  loaded automatically, and it is deliberately a toy, not a definitions
  database.

## A preset over a more general engine

Everything in this chapter is the behavior of the
[`latexlike`](crate::latexlike) preset, not of techy's parsing engine. The
engine knows nothing of `\`, `{`, `%`, math modes, or environments; it knows
tokens, groups as declared by token rules, parsing states, and **callables** —
the general concept of anything invocable by name from the source text
([callable specs](crate::guide::concepts_overview#callable-specs-and-arguments)).
The preset supplies the rest as configuration: LaTeX's tokenization as default
token rules, text and math as its two parsing modes, and — crucially — macros,
environments, and specials as its three *callable types*. "Macro" is not an
engine concept: it is the preset's name for one way a callable is invoked.

A different preset could make different choices with the same engine: other
tokenization rules, other modes, and other, orthogonal kinds of callables —
invocation forms that are not macros, environments, or specials at all. The
practical consequence for readers of this guide: when the documentation talks
about macros, environments, and specials, that is preset vocabulary, covered
by the User Guide; when you need a language whose concepts differ from the
preset's, the Developer Guide chapter
[Defining a custom language](crate::guide::custom_lang) shows how the same
engine machinery is configured from scratch — and the preset itself, built
entirely from public extension points, is the worked example.

Read next: [Node trees](crate::guide::node_trees) — what these constructs
become once parsed.
