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
//!   [`CallableData`] records the invocation facts, including the parsed
//!   [`ParsedArguments`]/[`ParsedSlots`] records and their [`ChildRegion`]s.
//! - **Building** — trees come out of a [`NodeTreeBuilder`] (staging
//!   [`BuildId`]s, contract violations reported as [`NodeBuildError`]);
//!   [`validate_tree`] checks any finished tree — parsed, restaged, or spliced —
//!   against the all-trees law, reporting the first [`TreeViolation`].
//!
//! Extraction helpers over parsed trees live in [`extract`](crate::extract).

pub use crate::node::{
    display_tree, validate_tree, ArgumentExt, BodySlotExt, BuildId, CallableData, ChildRegion,
    ContentNodes, Descendants, GroupData, NodeBuildError, NodeExt, NodeId, NodeKind,
    NodeRef, NodeSlice, NodeSliceIter, NodeTree, NodeTreeBuilder, ParsedArgument,
    ParsedArguments, ParsedSlot, ParsedSlots, SlotExt, SlotRole, StagedChildView,
    StagedChildren, StagedNodeView, StagedNodes, TreeTag, TreeViolation,
    TreeViolationKind,
};
