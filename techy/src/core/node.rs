//! The node tree: flat, frozen, index-based AST storage — reading, payloads, and
//! building.
//!
//! - **Reading** — a [`NodeTree`] stores all nodes of a parse in one flat vector and
//!   is only read afterwards, through [`NodeRef`] proxies, [`NodeSlice`] views, and
//!   the [`Descendants`] iterator. [`NodeKind`] is the closed structural core
//!   (`Chars` / `Group` / `Callable` / `Comment` / `List`); custom data rides in the
//!   ext system (the [`NodeExtTypes`](crate::core::NodeExtTypes) bundle).
//!   [`display_tree`] renders a subtree for human eyes.
//! - **Payloads** — [`GroupData`] records a group's delimiters and typed class;
//!   [`CommentData`] records a comment's start delimiter, content, and post-space;
//!   [`CallableData`] records the invocation facts, including the parsed
//!   [`ParsedArguments`]/[`ParsedSlots`] records and their [`ChildRegion`]s.
//! - **Building** — trees come out of a [`NodeTreeBuilder`] (staging
//!   [`BuildId`]s, contract violations reported as [`NodeBuildError`]);
//!   [`validate_tree`] checks any finished tree — parsed, restaged, or spliced —
//!   against the all-trees law, reporting the first [`TreeViolation`].
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
