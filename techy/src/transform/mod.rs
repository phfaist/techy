//! Tree→tree transformation: the **streaming restage** driver.
//!
//! A transformation pass walks a frozen input
//! [`NodeTree`](crate::core::node::NodeTree) **top-down** — the
//! [`RestageVisitor`] decides about every node *before* its subtree is committed
//! (a buried `\includegraphics` is seen before anything below it is staged) —
//! while the driver stages the output **bottom-up** into a fresh
//! [`NodeTreeBuilder`](crate::core::node::NodeTreeBuilder) (children before
//! parents; the driver mediates the two orders). The entry point is [`restage`]:
//!
//! ```
//! use techy::core::{Language, ParsingState};
//! use techy::core::node::{NodeId, NodeRef, NodeTree};
//! use techy::error::Recovery;
//! use techy::latexlike::{Latexlike, LatexlikeDriver};
//! use techy::transform::{restage, Restage, RestageContext, RestageError};
//!
//! // The origin-tracking convention: the consumer's own annotation type carries
//! // the original node's id (ids are tree-tagged, so old ids stay unambiguous).
//! #[derive(Clone, Debug)]
//! struct Ann { original: NodeId }
//!
//! let language: Language<Latexlike> = Language::new(
//!     LatexlikeDriver::new(Recovery::Strict),
//!     ParsingState::lang_initial().expect("seed state"),
//! );
//! let input = language.parse("a{b}c").unwrap().tree;
//!
//! // Restage every node unchanged, annotating each copy with its original:
//! let output: NodeTree<Latexlike, Ann> = restage(
//!     &input,
//!     &mut |node: NodeRef<'_, Latexlike>,
//!           _cx: &mut RestageContext<'_, Latexlike, (), Ann>| {
//!         Ok::<_, core::convert::Infallible>(
//!             Restage::Descend(Ann { original: node.id() }),
//!         )
//!     },
//! )
//! .unwrap();
//!
//! assert_eq!(output.root().annotation().original, input.root().id());
//! ```
//!
//! # The callback contract
//!
//! Per node the visitor returns a [`Restage`]:
//!
//! - [`Descend(b)`](Restage::Descend) — the driver restages this node over its
//!   children's results with annotation `b`, and **the visitor continues through
//!   every child subtree**. This is a safety invariant: the only way a child
//!   subtree goes unvisited is an explicit `Emit` for one of its ancestors —
//!   there is no shallow-keep to reach by accident.
//! - [`Emit(nodes)`](Restage::Emit) — the callback staged the replacement itself
//!   (through the [context](RestageContext)'s region ops or its raw
//!   [`builder()`](RestageContext::builder)); an empty vector drops the node.
//!   **No automatic descent**: whatever the callback wants restaged from the
//!   subtree, it restages explicitly (e.g.
//!   [`restage_children`](RestageContext::restage_children)).
//!
//! Descent is **structural, never role-conditional**: the driver walks into
//! [`Attached`](crate::core::node::SlotRole::Attached) *and*
//! [`Hidden`](crate::core::node::SlotRole::Hidden) slot children exactly like
//! into any other child — protective verbatim treatment of attached content is
//! one explicit visitor arm, not driver behavior.
//!
//! # Read frozen, write staged
//!
//! Callbacks *inspect the frozen input* — the full read API and the
//! [`techy::extract`](crate::extract) tools — and *produce* staged output; the
//! staged side is write-only ([`BuildId`]s and
//! bundles). A `Descend` parent never sees its children's results: decisions
//! precede restaging (top-down), and whatever a callback stages it just made —
//! facts carry in closure state or annotations. A framework that must inspect
//! transform output finishes the tree and runs another pass; multi-stage
//! pipelines are deliberately cheap (zero-copy
//! [`annotate`](crate::core::node::NodeTree::annotate), `Arc`-shared payloads).
//!
//! # Annotations: one pathway
//!
//! *Every* restaged node's annotation passes through the visitor — as
//! `Descend(b)`, or as an explicit argument to the staging ops the callback
//! invokes. The input and output annotation types are different type parameters,
//! so "keep the annotation" is deliberately not expressible; the origin-id
//! convention is the one-liner `Descend(Ann { original: node.id(), .. })` (see
//! the example above — "original node" is the vocabulary; "provenance" and
//! "origin" alone belong to the source model). To find the *new* node minted for
//! an old id, walk the finished tree's annotations once — the O(n) inversion:
//!
//! ```text
//! let new_of_old: HashMap<NodeId, NodeId> = output
//!     .iter_storage_order()
//!     .map(|n| (n.annotation().original, n.id()))
//!     .collect();
//! ```
//!
//! # Region edits: no silent repair
//!
//! The driver acts exactly like parse-time construction — legal shapes pass,
//! broken designations are errors:
//!
//! - Dropping every node of an argument's region restages the argument as
//!   **provided with an empty region** (absent ≠ empty is parser semantics;
//!   true absence is the explicit
//!   [`RestagedArgument::absent`]).
//! - Dropping (or multiplying) a node that anchors a record's
//!   [`InChildrenOf`](crate::core::node::ContentNodes::InChildrenOf) content
//!   designation is [`RestageError::ContentParentDropped`]: re-anchoring would
//!   silently change what the record *means*, and a multi-node replacement makes
//!   any auto-re-anchor ill-defined even in principle. The remedy is a takeover:
//!   `Emit` the callable's replacement yourself (via
//!   [`restage_invocation`](RestageContext::restage_invocation) or the raw
//!   builder).
//!
//! # Cross-tree by contract
//!
//! The context ops (and the level-0
//! [`NodeTreeBuilder::restage_node`](crate::core::node::NodeTreeBuilder::restage_node)
//! primitive underneath them) accept nodes from **any** tree, not just the run's
//! input — cross-tree input is supported by contract, for assembling a new tree
//! out of pieces of several others.

use core::fmt;

use alloc::string::String;
use alloc::vec::Vec;

use crate::node::{BuildId, NodeBuildError, NodeId, NodeRef};
use crate::state::Lang;

mod bundles;
mod context;

pub use bundles::{RestagedArgument, RestagedSlot};
pub use context::{restage, RestageContext};

/// The visitor's verdict about one input node (returned from
/// [`RestageVisitor::restage`]).
#[derive(Clone, Debug)]
pub enum Restage<B> {
    /// The driver restages this node over its children's results, with the given
    /// annotation — and the visitor **always** continues through every child
    /// subtree (the safety invariant: no shallow-keep exists; see the
    /// [module docs](self)).
    Descend(B),
    /// The callback staged the replacement itself; these are the staged nodes
    /// standing in for this node (empty = drop). No automatic descent happens.
    Emit(Vec<BuildId>),
}

/// A restage callback: invoked once per visited node, top-down, with the frozen
/// input node in hand and the staging [`RestageContext`] to write through.
///
/// This is a trait (not a bare closure parameter) because the region ops re-enter
/// the visitor from *inside* a visitor call — `cx.restage_argument(node, 0, self)`
/// — and a closure cannot pass itself. For non-reentrant passes, any
/// `FnMut(NodeRef<'_, L, A>, &mut RestageContext<'_, L, A, B>) ->
/// Result<Restage<B>, E>` closure is a visitor via the blanket impl. One
/// inference note: an **inline closure must annotate its two parameter types**
/// (`&mut |node: NodeRef<'_, L, A>, cx: &mut RestageContext<'_, L, A, B>| { … }`
/// — the [module](self) doctest models the spelling; a fully unannotated
/// `|node, cx|` does not infer against the generic visitor parameter), while a
/// fn item needs no annotations.
///
/// Deliberately **no `Send`/`Sync` bounds** (here and on
/// [`annotate`](crate::core::node::NodeTree::annotate) callbacks): the driver
/// runs visitors synchronously on the calling thread, so such a bound would be a
/// demand on callers buying nothing — and it would exclude single-threaded
/// foreign-function (FFI)
/// callbacks. Parallel variants, if ever wanted, are new entry points with their
/// own bounds (the `&mut self` contract is inherently serial).
///
/// # Errors
///
/// `Error` is the visitor's own failure type; it rides through [`restage`] typed,
/// as [`RestageError::Visitor`]. Context ops fail with
/// [`RestageError`]`<Self::Error>` values; a visitor that wants to propagate
/// them with `?` gives its error type a conversion, e.g.:
///
/// ```text
/// enum PassError {
///     Restage(Box<RestageError<PassError>>),  // op failures, boxed
///     BadReference(String),                   // the pass's own conditions
/// }
/// impl From<RestageError<PassError>> for PassError { … }
/// ```
pub trait RestageVisitor<L: Lang, A, B> {
    /// The visitor's own error type (see the trait docs).
    type Error;

    /// Decide this node's restaging (see [`Restage`] and the [module docs](self)).
    fn restage(
        &mut self,
        node: NodeRef<'_, L, A>,
        cx: &mut RestageContext<'_, L, A, B>,
    ) -> Result<Restage<B>, Self::Error>;
}

/// Closures are visitors (non-reentrant passes; see [`RestageVisitor`]'s docs).
impl<L: Lang, A, B, E, F> RestageVisitor<L, A, B> for F
where
    F: FnMut(NodeRef<'_, L, A>, &mut RestageContext<'_, L, A, B>) -> Result<Restage<B>, E>,
{
    type Error = E;

    fn restage(
        &mut self,
        node: NodeRef<'_, L, A>,
        cx: &mut RestageContext<'_, L, A, B>,
    ) -> Result<Restage<B>, E> {
        self(node, cx)
    }
}

/// Error of a [`restage`] run — generic over the visitor's own error type `E`
/// (the framework's error rides through typed; `Clone`/`PartialEq`/`Eq` are
/// conditional on `E`, keeping the uniform-Clone principle).
///
/// The variants beyond the visitor's own ([`Visitor`](RestageError::Visitor))
/// report either builder contract violations
/// ([`Build`](RestageError::Build)), driver-detected unrepairable edits
/// ([`ContentParentDropped`](RestageError::ContentParentDropped)), or misuse of
/// a context op (a documented-contract violation returns an `Err`, never
/// panics).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RestageError<E> {
    /// Staging into the output builder failed ([`NodeBuildError`] — the staged
    /// input violated the builder's contract; the run is abandoned).
    Build(NodeBuildError),
    /// A restaged callable's argument/slot record designates its content inside
    /// a node ([`InChildrenOf`](crate::core::node::ContentNodes::InChildrenOf))
    /// that the pass dropped or did not restage one-to-one — re-anchoring is
    /// ill-defined, so the driver refuses instead of silently changing what the
    /// record means. Take over the callable itself: `Emit` its replacement,
    /// staged via [`restage_invocation`](RestageContext::restage_invocation) or
    /// the raw [`builder()`](RestageContext::builder).
    ContentParentDropped {
        /// The callable whose record lost its anchor.
        callable: NodeId,
        /// The content-parent node (in the input tree) the record designates.
        parent: NodeId,
        /// How many staged nodes replaced the parent: `Some(0)` = dropped,
        /// `Some(2)`… = multiplied; `None` = never individually restaged (an
        /// ancestor was taken over via `Emit`, so no per-node replacement was
        /// recorded).
        replaced_by: Option<usize>,
    },
    /// The visitor itself failed; its error, verbatim.
    Visitor(E),
    /// [`restage_argument_named`](RestageContext::restage_argument_named) was
    /// given a name that matches none of the callable's argument specs.
    UnknownArgumentName {
        /// The callable that was queried.
        node: NodeId,
        /// The unmatched argument name.
        name: String,
    },
    /// An argument index beyond the callable's argument record.
    ArgumentIndexOutOfRange {
        /// The callable that was queried.
        node: NodeId,
        /// The out-of-range index.
        index: usize,
        /// The callable's argument count.
        count: usize,
    },
    /// A slot index beyond the callable's slot record.
    SlotIndexOutOfRange {
        /// The callable that was queried.
        node: NodeId,
        /// The out-of-range index.
        index: usize,
        /// The callable's slot count.
        count: usize,
    },
    /// An argument/slot/invocation op was applied to a node that is not a
    /// `Callable`.
    NotACallable {
        /// The offending node.
        node: NodeId,
    },
    /// [`restage_argument_with_content`](RestageContext::restage_argument_with_content)
    /// was applied to an argument that was not provided: an absent argument has
    /// no wrapper syntax to restage a new content into — construct the bundle
    /// explicitly via [`RestagedArgument::provided`] instead.
    ArgumentAbsent {
        /// The callable that was queried.
        node: NodeId,
        /// The absent argument's index.
        index: usize,
    },
    /// The root's replacement was not exactly one staged node ([`restage`]
    /// returns a tree, and a synthesized wrapper would need an annotation the
    /// driver cannot invent — wrap the root yourself via `Emit`).
    RootNotSingular {
        /// How many staged nodes the visitor produced for the root.
        count: usize,
    },
}

impl<E: fmt::Display> fmt::Display for RestageError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestageError::Build(error) => {
                write!(f, "staging the restaged tree failed: {}", error)
            }
            RestageError::ContentParentDropped { callable, parent, replaced_by } => {
                match replaced_by {
                    Some(0) => write!(
                        f,
                        "content parent {:?} of callable {:?}'s argument/slot record was \
                         dropped",
                        parent, callable
                    )?,
                    Some(n) => write!(
                        f,
                        "content parent {:?} of callable {:?}'s argument/slot record was \
                         replaced by {} nodes",
                        parent, callable, n
                    )?,
                    None => write!(
                        f,
                        "content parent {:?} of callable {:?}'s argument/slot record has \
                         no recorded one-to-one replacement",
                        parent, callable
                    )?,
                }
                write!(
                    f,
                    " — re-anchoring the record is ill-defined; take over the callable \
                     (Emit its replacement, staged via restage_invocation or the raw \
                     builder)"
                )
            }
            RestageError::Visitor(error) => write!(f, "restage visitor failed: {}", error),
            RestageError::UnknownArgumentName { node, name } => write!(
                f,
                "callable {:?} has no argument named {:?}",
                node, name
            ),
            RestageError::ArgumentIndexOutOfRange { node, index, count } => write!(
                f,
                "callable {:?} has no argument #{} ({} arguments)",
                node, index, count
            ),
            RestageError::SlotIndexOutOfRange { node, index, count } => write!(
                f,
                "callable {:?} has no slot #{} ({} slots)",
                node, index, count
            ),
            RestageError::NotACallable { node } => {
                write!(f, "node {:?} is not a callable", node)
            }
            RestageError::ArgumentAbsent { node, index } => write!(
                f,
                "argument #{} of callable {:?} is absent: there is no wrapper syntax to \
                 restage new content into (construct RestagedArgument::provided instead)",
                index, node
            ),
            RestageError::RootNotSingular { count } => write!(
                f,
                "the root restaged to {} nodes; a tree needs exactly one root (wrap the \
                 replacement in a node of your own via Emit)",
                count
            ),
        }
    }
}

impl<E> core::error::Error for RestageError<E>
where
    E: core::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            RestageError::Build(error) => Some(error),
            RestageError::Visitor(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
