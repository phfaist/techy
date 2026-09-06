//! The parsed syntax tree: how nodes are stored and read, the payload each node
//! kind carries, and how trees are built and checked.
//!
//! - **Reading** — a [`NodeTree`] holds every node of one parse in a single flat
//!   array and is immutable once finished. Read it through [`NodeRef`] proxies,
//!   [`NodeSlice`] views over a run of children, and the [`Descendants`] iterator;
//!   [`display_tree`] renders a subtree as indented text for inspection.
//!   [`NodeKind`] is the closed set of structural kinds (`Chars` / `Group` /
//!   `Callable` / `Comment` / `List`), and it does not grow: a language attaches
//!   data of its own to each node, parsed argument, and parsed slot through the
//!   extension types it declares ([`NodeExtTypes`](crate::core::NodeExtTypes)), and
//!   a node carrying such data is still an ordinary node of its kind to code that
//!   reads the tree generically.
//! - **Payloads** — [`GroupData`] records a group's delimiters and typed class;
//!   [`CommentData`] records a comment's start delimiter, content, and post-space;
//!   [`CallableData`] records the invocation facts, including the parsed
//!   [`ParsedArguments`]/[`ParsedSlots`] records and their [`ChildRegion`]s.
//! - **Building** — trees come out of a [`NodeTreeBuilder`], which stages nodes
//!   under [`BuildId`]s and reports contract violations as [`NodeBuildError`].
//!   [`validate_tree`] checks a finished tree of any origin — parsed, restaged, or
//!   spliced together — against the invariants every tree must satisfy, and returns
//!   the first [`TreeViolation`] it finds.
//!
//! Working with a finished tree beyond direct reads: read-only structural
//! traversal is [`visit`](crate::visit)
//! ([`TreeWalker`](crate::visit::TreeWalker)),
//! extraction helpers live in [`extract`](crate::extract), tree→tree
//! transformation is [`transform`](crate::transform)
//! ([`TreeRestager`](crate::transform::TreeRestager)), and tree→value
//! recomposition is [`recompose`](crate::recompose)
//! ([`TreeRecomposer`](crate::recompose::TreeRecomposer)). The narrative
//! introduction to all of these is the guide's node-trees chapter
//! ([`guide::node_trees`](crate::guide::node_trees)).

pub use crate::node::{
    display_tree, validate_tree, ArgumentExt, BodySlotExt, BuildId, CallableData, ChildRegion,
    CommentData, ContentNodes, Descendants, GroupData, NamedAccessError, NodeBuildError, NodeExt,
    NodeId, NodeKind, NodeRef, NodeSlice, NodeSliceIter, NodeTree, NodeTreeBuilder,
    ParsedArgument, ParsedArguments, ParsedSlot, ParsedSlots, SlotExt, SlotRole,
    StagedChildView, StagedChildren, StagedNodeView, StagedNodes, TreeTag,
    TreeViolation, TreeViolationKind,
};
