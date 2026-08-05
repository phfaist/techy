# Node trees

A parse produces a **node tree**: a structured representation of the source
text that your program reads, searches, transforms, and converts. This chapter
introduces the tree itself — what kinds of nodes exist and how to read them —
and then tours what techy can do with a parsed tree. The chapter stays
high-level on purpose: each tool's module documentation is the reference for
actually using it, with worked examples.

## What a parse produces

Running a parse (see [Running the parser](crate::guide::parsing)) yields a
[`ParseResult`](crate::core::ParseResult) carrying the
[`NodeTree`](crate::core::node::NodeTree) and the diagnostics collected along
the way. The tree is *flat and frozen*: all nodes of the parse live in one
indexed store, and the tree is only read after the parse — there is no
in-place mutation. Changing a tree means producing a new one (see
[transformation](#transforming-trees-techytransform) below).

You read the tree through lightweight handles:
[`NodeRef`](crate::core::node::NodeRef) is a reference to one node, and
[`NodeSlice`](crate::core::node::NodeSlice) is a view of a run of sibling
nodes. Every node records its exact byte range in the source (its
[span](crate::guide::concepts_overview#sources-and-spans) — the original text
is always reachable via
[`span_content()`](crate::core::node::NodeRef::span_content)) and the
[parsing state](crate::guide::concepts_overview#parsing-state-and-deltas) it
was parsed under (so you can ask, for example, whether a node sits in math
mode).

## The node kinds

Node structure is a small, closed set — the
[`NodeKind`](crate::core::node::NodeKind) enum:

- **Characters** (`Chars`) — a run of ordinary content characters, including
  whitespace-only runs.
- **Group** (`Group`) — a delimited group (`{…}`, `$…$`); the payload records
  the delimiters as written and the group's class, and the node's children
  hold the group's contents ([`GroupData`](crate::core::node::GroupData)).
- **Callable invocation** (`Callable`) — an invocation of a
  [callable](crate::guide::concepts_overview#callable-specs-and-arguments):
  in latexlike terms, a macro, environment, or specials use. The payload
  records the name, the invocation form, the definition that parsed it, the
  parsed arguments with their content regions, and the invocation's spelling
  facts ([`CallableData`](crate::core::node::CallableData)). There are
  deliberately no separate "macro node" or "environment node" kinds: those
  differ by invocation form, not by parsed shape, and the form is data on the
  one `Callable` kind.
- **Comment** (`Comment`) — a comment; the start delimiter, the comment text,
  and the trailing newline-plus-indentation are each recorded.
- **List** (`List`) — a plain sequence of nodes: the tree root, an
  environment's body, or a multi-node argument value.

Reading is direct: kind predicates
([`is_group()`](crate::core::node::NodeRef::is_group), …), kind-specific
accessors ([`chars()`](crate::core::node::NodeRef::chars),
[`group_delimiters()`](crate::core::node::NodeRef::group_delimiters),
[`name()`](crate::core::node::NodeRef::name),
[`arguments()`](crate::core::node::NodeRef::arguments),
[`body()`](crate::core::node::NodeRef::body), …), child navigation
([`children()`](crate::core::node::NodeRef::children),
[`parent()`](crate::core::node::NodeRef::parent)), and the document-order
iterator [`descendants()`](crate::core::node::NodeRef::descendants). For
latexlike trees, the preset adds vocabulary-specific accessors on the same
`NodeRef` type ([`macro_name()`](crate::core::node::NodeRef::macro_name),
[`environment_name()`](crate::core::node::NodeRef::environment_name),
[`is_math_group()`](crate::core::node::NodeRef::is_math_group), …).

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::minidefs::minilatex_package;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([minilatex_package()]),
);
let result = language.parse(r"one \emph{two} % three").unwrap();

// The root is a List node covering the whole input.
let root = result.tree.root();
assert!(root.is_list());
assert_eq!(root.child_count(), 4);

// Kind predicates and kind-specific accessors:
let chars = root.child(0).unwrap();
assert!(chars.is_chars());
assert_eq!(chars.chars(), Some("one "));

let emph = root.child(1).unwrap();
assert!(emph.is_callable());
assert_eq!(emph.name(), Some("emph"));       // any callable's name
assert_eq!(emph.macro_name(), Some("emph")); // preset sugar: macros only
assert_eq!(
    emph.argument_content_nodes(0).unwrap().source_text(),
    Some("two"),
);

let comment = root.child(3).unwrap();
assert!(comment.is_comment());
assert_eq!(comment.comment(), Some(" three"));

// Every node knows exactly where it came from:
assert_eq!(emph.span_content(), r"\emph{two}");
assert_eq!(emph.span().range(), 4..14);

// Document-order iteration over everything beneath a node:
let texts: Vec<&str> = root.descendants().filter_map(|n| n.chars()).collect();
assert_eq!(texts, ["one ", "two", " "]);
```

For debugging, [`summary()`](crate::core::node::NodeRef::summary) renders a
compact one-line description of a node, and
[`display_tree`](crate::core::node::display_tree) renders a whole subtree.

The rest of this chapter is the tour: four modules, each consuming parsed
trees a different way.

## Extracting content: `techy::extract`

The [`extract`](crate::extract) module answers the everyday "give me the
*text*" questions over parsed node lists: flattening content to a plain
string, splitting a node list at a separator character with grouped content
protected (`\cite{a,b{x,y},c}`), and reading `key=value` lists. The splitting
helpers assemble real trees as output, so their results compose with every
other tree consumer. Entry points, the annotation-callback forms, and worked
examples are in the [`extract`](crate::extract) module documentation.

## Traversing: `techy::visit`

The [`visit`](crate::visit) module is read-only structural traversal:
[`walk`](crate::visit::walk) drives a
[`NodeVisitor`](crate::visit::NodeVisitor) over a subtree in document order,
with an enter/exit call pair around each node's children and per-node control
over the traversal (descend, skip the children, or stop). Use it when the
flat [`descendants()`](crate::core::node::NodeRef::descendants) iterator is
not enough — when your pass needs to know about nesting, or needs a hook
after a node's children. The contract and examples are in the
[`visit`](crate::visit) module documentation.

## Transforming trees: `techy::transform`

The [`transform`](crate::transform) module is tree-to-tree transformation.
Because trees are frozen, editing is expressed as **restaging**:
[`restage`](crate::transform::restage) walks the input tree and stages a new
tree, with a visitor deciding per node whether to carry it over (descending
into its children) or to emit a replacement — drop a node, rewrite it,
splice in nodes from another tree. Output nodes carry a consumer-chosen
annotation type, which is also the idiom for tracking which input node an
output node came from. The callback contract, the region-editing operations,
and worked examples are in the [`transform`](crate::transform) module
documentation.

## Composing a value: `techy::recompose`

The [`recompose`](crate::recompose) module folds a tree into a single value —
plain text, HTML, a token stream, anything you can concatenate:
[`recompose`](crate::recompose::recompose) drives a
[`Recomposer`](crate::recompose::Recomposer) that answers one instruction per
node (emit this piece, or concatenate the children's pieces). Re-emitting a
tree's exact source spelling is one shipped recomposer, the preset's
[`source_recomposer`](crate::latexlike::source_recomposer) — the natural
finish of a transformation pipeline: restage the tree, then recompose the
result back to source text. The instruction vocabulary, the state threading,
and worked examples are in the [`recompose`](crate::recompose) module
documentation.

## Choosing between them

- Just need text out of an argument or a list? — [`extract`](crate::extract).
- Scanning or analyzing, without producing a new tree? —
  [`descendants()`](crate::core::node::NodeRef::descendants) for flat
  queries, [`visit`](crate::visit) for structure-aware passes.
- Changing the document? — [`transform`](crate::transform) to produce the
  new tree, then [`recompose`](crate::recompose) (with the preset's
  [`source_recomposer`](crate::latexlike::source_recomposer)) to render it
  back to source text.
- Converting to another format? — [`recompose`](crate::recompose) with your
  own [`Recomposer`](crate::recompose::Recomposer).

Read next: [Defining macros, environments, and
specials](crate::guide::specs) — the definitions that shape what these trees
contain.
