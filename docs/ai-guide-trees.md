# AI guide: node trees

Condensed reference: reading, navigating, extracting from, transforming,
and recomposing parsed node trees. Compressed from
[Node trees](crate::guide::node_trees) and the
[`extract`](crate::extract) / [`visit`](crate::visit) /
[`transform`](crate::transform) / [`recompose`](crate::recompose) module
documentation (the contract references). Terms: a parse produces a
[`NodeTree`](crate::core::node::NodeTree) — flat (all nodes in one indexed
store) and frozen (read-only after the parse; changing a tree means
producing a new one). [`NodeRef`](crate::core::node::NodeRef) is a
borrowed reference to one node; [`NodeSlice`](crate::core::node::NodeSlice)
a view of a run of sibling nodes. A **span** is a node's exact byte range
into its source; a **callable** is anything invoked by name (macro,
environment, specials); an **annotation** is a consumer-chosen per-node
value on trees produced by the consumer toolkit.

## Node kinds

The closed [`NodeKind`](crate::core::node::NodeKind):

| Kind | Content | Payload |
|---|---|---|
| `Chars` | run of ordinary characters (incl. whitespace-only runs) | text |
| `Group` | delimited group (`{…}`, `$…$`); children = contents | [`GroupData`](crate::core::node::GroupData): delimiters as written + group class |
| `Callable` | macro/environment/specials invocation — one kind; the form is data, not a node class | [`CallableData`](crate::core::node::CallableData): name, invocation form, spec, parsed arguments, content regions, spelling facts |
| `Comment` | a comment | start delimiter, text, trailing newline+indentation, each recorded |
| `List` | plain sequence: tree root, environment body, multi-node argument value | — |

## Reading and navigating

```rust
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::minidefs::minilatex_package;
use techy::latexlike::{Latexlike, LatexlikeDriver};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([minilatex_package()]).expect("seed state"),
);
let result = language.parse(r"one \emph{two} % three").unwrap();
let root = result.tree.root();          // root is a List over the whole input
assert!(root.is_list());

// Kind predicates + kind-specific accessors:
assert_eq!(root.child(0).unwrap().chars(), Some("one "));
let emph = root.child(1).unwrap();
assert!(emph.is_callable());
assert_eq!(emph.name(), Some("emph"));       // any callable
assert_eq!(emph.macro_name(), Some("emph")); // latexlike sugar: macros only
assert_eq!(emph.argument_content_nodes(0).unwrap().source_text(), Some("two"));
assert_eq!(root.child(3).unwrap().comment(), Some(" three"));

// Spans: every node records its exact byte range; source text needs no lookup.
assert_eq!(emph.span_content(), r"\emph{two}");
assert_eq!(emph.span().range(), 4..14);

// Flat document-order iteration beneath a node (excludes the node itself):
let texts: Vec<&str> = root.descendants().filter_map(|n| n.chars()).collect();
assert_eq!(texts, ["one ", "two", " "]);
```

Navigation summary (details on each method's page):

| Need | Use |
|---|---|
| children / parent | [`children()`](crate::core::node::NodeRef::children), [`child(i)`](crate::core::node::NodeRef::child), [`parent()`](crate::core::node::NodeRef::parent) |
| flat scan, no nesting info | [`descendants()`](crate::core::node::NodeRef::descendants) |
| structure-aware pass (enter/exit, depth, skip/stop) | [`visit::walk`](crate::visit::walk) |
| deepest node at a byte position | [`NodeTree::node_at`](crate::core::node::NodeTree::node_at) |
| minimal sibling run covering a span | [`NodeTree::covering_slice`](crate::core::node::NodeTree::covering_slice) |
| environment body / marked body slot | [`body()`](crate::core::node::NodeRef::body) — `None` for non-callables and body-less callables; `Some` with zero nodes = empty body |
| argument content | [`argument_content_nodes(i)`](crate::core::node::NodeRef::argument_content_nodes) / [`argument_content_nodes_named(name)`](crate::core::node::NodeRef::argument_content_nodes_named) |
| parsing state a node was parsed under (e.g. mode) | [`parsing_state()`](crate::core::node::NodeRef::parsing_state) |
| debug rendering | [`summary()`](crate::core::node::NodeRef::summary) (one line), [`display_tree`](crate::core::node::display_tree) (subtree) |

## Extracting content: `techy::extract`

Free functions answering "give me the text". Reader:
[`content_as_chars`](crate::extract::content_as_chars) flattens chars and
groups to a `String` (fails honestly on non-text). Builders (four
producers): [`split_at_chars`](crate::extract::split_at_chars) (split a
node list at a separator character, grouped content protected),
[`parse_keyval`](crate::extract::parse_keyval) (`key=value,…` lists), and
the argument-run readers
[`split_embellishments`](crate::extract::split_embellishments) /
[`split_tack_on_fields`](crate::extract::split_tack_on_fields). Builders
mint a **new tree** (the input is frozen; boundary partials become fresh
span-backed `Chars` nodes), so their segments are ordinary
[`NodeSlice`](crate::core::node::NodeSlice) views and compose with every
other tree consumer.

Each producer ships three spellings: the bare name takes an
annotation-minting callback (called once per staged output node with a part
context: the original input node, cut-piece facts, segment index);
`*_drop_annotations` is the plain-output shorthand (`B = ()`);
`*_keep_annotations` clones input annotations through. Callbacks mint
annotations only — modifying nodes is [`transform`](crate::transform)'s
job.

```rust
use techy::core::{Language, ParsingState};
use techy::core::specs::Package;
use techy::error::Recovery;
use techy::extract;
use techy::latexlike::{argument_specs, CallableType, Latexlike, LatexlikeDriver, MacroSpec};

let mut package = Package::new("mydefs");
package.define_macro("usetikzlibrary", ["m"]).unwrap();
package.insert(
    CallableType::Macro,
    "includegraphics",
    MacroSpec::new(argument_specs(["o", "m"]).unwrap()),
);
let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial_with_packages([package]).expect("seed state"),
);

// Split a comma-separated argument, groups protected:
let result = language.parse(r"\usetikzlibrary{arrows,shapes.geometric,calc}").unwrap();
let list = result.tree.root().child(0).unwrap().argument_content_nodes(0).unwrap();
assert_eq!(extract::content_as_chars(list).unwrap(), "arrows,shapes.geometric,calc");
let split = extract::split_at_chars_drop_annotations(list, ",").unwrap();
let libraries: Vec<&str> =
    split.segments().map(|segment| segment.source_text().unwrap()).collect();
assert_eq!(libraries, ["arrows", "shapes.geometric", "calc"]);

// Keyval options, grouped values protected:
let result = language
    .parse(r"\includegraphics[width=5cm,label={fig,main}]{fig.pdf}")
    .unwrap();
let node = result.tree.root().child(0).unwrap();
let keyvals =
    extract::parse_keyval_drop_annotations(node.argument_content_nodes(0).unwrap()).unwrap();
assert_eq!(keyvals.get("width").unwrap().value().unwrap().source_text(), Some("5cm"));
// value_content() sees inside a grouped value's braces:
assert_eq!(keyvals.get("label").unwrap().value_content().unwrap().source_text(),
           Some("fig,main"));
```

## Traversing: `techy::visit`

[`walk`](crate::visit::walk) drives a
[`NodeVisitor`](crate::visit::NodeVisitor) over a subtree in document order
(preorder: `enter` before the children, `exit` after), returning a
[`VisitFlow`](crate::visit::VisitFlow) per node: descend, skip the
children, or stop the walk. A plain closure is a visitor (enter-only). The
walk is **role-blind**: it visits children in `Attached` and `Hidden` slot
regions like any other child (contrast recompose below). Consumer state
lives in the visitor's own `&mut self` fields — the
[`VisitContext`](crate::visit::VisitContext) carries only depth and tree
access; a walk needing *scoped downward* state is a
[`Recomposer`](crate::recompose::Recomposer) with `Piece = ()` (the
three-channel state discipline, [`visit`](crate::visit) module docs).

## Transforming: `techy::transform`

Trees are frozen, so editing is **restaging**:
[`restage`](crate::transform::restage) walks the input top-down while
staging a new tree bottom-up. Per node the
[`RestageVisitor`](crate::transform::RestageVisitor) returns a
[`Restage`](crate::transform::Restage):

| Return | Effect |
|---|---|
| `Restage::Descend(b)` | carry the node over (restaged over its children's results) with output annotation `b`; the visitor continues through every child subtree — no accidental shallow-keep |
| `Restage::Emit(nodes)` | the callback staged the replacement itself (region ops on [`RestageContext`](crate::transform::RestageContext), or the raw builder); empty vector = drop the node; **no automatic descent** — restage wanted subtree parts explicitly (e.g. [`restage_children`](crate::transform::RestageContext::restage_children)) |

Facts to know (all from the [`transform`](crate::transform) module docs):
callbacks read the frozen input and write staged output — a `Descend`
parent never sees its children's results; input and output annotation
types are different type parameters, and the **original-node idiom** is
`Descend(Ann { original: node.id() })` (invert once over the finished tree
for an old-id → new-id map). Region edits are checked, never silently
repaired: dropping every node of an argument's region restages it as
provided-but-empty (true absence is the explicit
[`RestagedArgument::absent`](crate::transform::RestagedArgument::absent));
dropping (or multiplying) a node that anchors an
[`InChildrenOf`](crate::core::node::ContentNodes::InChildrenOf) content
designation is
[`RestageError::ContentParentDropped`](crate::transform::RestageError) —
the remedy is an explicit `Emit` takeover
([`restage_invocation`](crate::transform::RestageContext::restage_invocation)).
Context ops accept nodes from **any** tree — the supported route for
assembling a tree from pieces of several others. Descent is structural,
never role-conditional.

## Recomposing: `techy::recompose`

[`recompose`](crate::recompose::recompose) folds a tree into one value
(text, HTML, tokens — anything you can concatenate). A
[`Recomposer`](crate::recompose::Recomposer) answers one
[instruction](crate::recompose::Recompose) per node: **`Emit(piece)`**
(this node's own result) or **`Concat`** (concatenate the children's
pieces; optional head/separator/tail via
[`ConcatPieces`](crate::recompose::ConcatPieces)); the driver folds
bottom-up. Downward context (math depth, output mode) is the recomposer's
`State`, threaded by argument and rederivable per `Concat`
([`with_state`](crate::recompose::ConcatPieces::with_state)); run-spanning
facts live in the recomposer's `&mut self`.

Facts to know (all from the [`recompose`](crate::recompose) module docs):

- **`Concat` default scope skips `Attached` and `Hidden` slot children**
  (opt in via [`include_attached`](crate::recompose::ConcatPieces::include_attached) /
  [`include_hidden`](crate::recompose::ConcatPieces::include_hidden)):
  `Attached` is derived content whose invocation text is its own
  recomposition (`\input`'s resolved file); `Hidden` means no
  recomposition. Recompose is the one role-sensitive site; reads (walk,
  descendants) stay role-blind.
- **Wrapping**: the driver holds one recomposer and every `Concat` descent
  re-enters it — a wrapping recomposer (override some nodes, delegate the
  rest) sees its overrides applied at every depth. Targeted replacement of
  *recomposition* is this pattern; targeted replacement of *content* is
  transform-then-recompose.
- **Streaming**: no sink type exists; a streaming recomposer holds its
  writer in `&mut self` and composes `Piece = ()`.
- **Reconstruction doctrine** (the reading contract): a recomposer
  reconstructs each node from the node's **own recorded payload** only.
  Resolving span content against the source — the node's own span included
  — and inter-node span arithmetic are forbidden: spans are provenance
  (where a node came from), not output location; "apparent gaps" between
  siblings would resurrect deleted content on any transformed tree.
  Byte-exact re-emission rests on payload completeness (the preset records
  invocation spelling in
  [`CallableData::invocation_syntax`](crate::core::node::CallableData::invocation_syntax)).

Source re-emission is one shipped recomposer — the preset's
[`source_recomposer`](crate::latexlike::source_recomposer) — byte-exact for
parsed trees; the core-complete building block is
[`core_source_instruction`](crate::recompose::core_source_instruction).

## The edit pipeline: restage → recompose

```rust
use techy::core::node::NodeRef;
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{source_recomposer, Latexlike, LatexlikeDriver};
use techy::recompose::recompose;
use techy::transform::{restage, Restage, RestageContext};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Strict),
    ParsingState::lang_initial().expect("seed state"),
);
let input = language.parse("one % secret\ntwo {three}").unwrap().tree;

// Re-emission reads recorded facts only — byte-exact for parsed trees:
let full = recompose(&input, (), &mut source_recomposer()).unwrap();
assert_eq!(full, "one % secret\ntwo {three}");

// Drop every comment; carry everything else over:
let cleaned = restage(
    &input,
    &mut |node: NodeRef<'_, Latexlike>,
          _cx: &mut RestageContext<'_, Latexlike, (), ()>| {
        Ok::<_, core::convert::Infallible>(if node.is_comment() {
            Restage::Emit(Vec::new()) // emit nothing: drop
        } else {
            Restage::Descend(())
        })
    },
)
.unwrap();
let stripped = recompose(&cleaned, (), &mut source_recomposer()).unwrap();
assert_eq!(stripped, "one two {three}");
```

## Choosing a consumer

| Goal | Tool |
|---|---|
| text out of an argument or list | [`extract`](crate::extract) |
| flat query / scan | [`descendants()`](crate::core::node::NodeRef::descendants) |
| structure-aware analysis pass | [`visit::walk`](crate::visit::walk) |
| change the document | [`transform::restage`](crate::transform::restage), then [`recompose`](crate::recompose::recompose) with [`source_recomposer`](crate::latexlike::source_recomposer) |
| convert to another format | [`recompose`](crate::recompose) with your own [`Recomposer`](crate::recompose::Recomposer) |
