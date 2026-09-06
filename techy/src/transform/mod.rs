//! Tree-to-tree transformation: produce a new node tree from an existing one.
//!
//! A parsed [`NodeTree`](crate::core::node::NodeTree) is frozen, so an edit is
//! expressed as *restaging*: the driver reads the input tree and stages a copy of
//! each node into a fresh
//! [`NodeTreeBuilder`](crate::core::node::NodeTreeBuilder), applying the changes
//! the caller asked for along the way.
//!
//! The two items to know are a driver and a callback. You implement a
//! [`RestageVisitor`] — a closure is one — and pass it to the driver
//! [`TreeRestager`]: `TreeRestager::new(&mut visitor).restage(&tree)` calls the
//! visitor once per input node, starting at the root and working downward, and
//! returns the finished output tree. Between two visitor calls the driver stages
//! the nodes the callback asked for, translating each callable's argument and slot
//! records onto the new child layout; a node is staged only after all of its
//! children are, so a decision about a node is always taken before anything below
//! it exists in the output.
//!
//! This module is one of three tree consumers, which differ in what they produce:
//! [`visit`](crate::visit) produces nothing (a read-only traversal),
//! [`recompose`](crate::recompose) produces a single value such as a `String`, and
//! this one produces a new tree. [Node trees](crate::guide::node_trees) compares
//! the three on one page.
//!
//! ```
//! use techy::core::{Language, ParsingState};
//! use techy::core::node::{NodeId, NodeRef, NodeTree};
//! use techy::error::Recovery;
//! use techy::latexlike::{Latexlike, LatexlikeDriver};
//! use techy::transform::{Restage, RestageContext, RestageError, TreeRestager};
//!
//! // To record where an output node came from, the consumer's own annotation
//! // type stores the input node's id (ids are tagged with the tree that minted
//! // them, so an input id stays unambiguous inside an output annotation).
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
//! let output: NodeTree<Latexlike, Ann> = TreeRestager::new(
//!     &mut |node: NodeRef<'_, Latexlike>,
//!           _cx: &mut RestageContext<'_, Latexlike, (), Ann>| {
//!         Ok::<_, core::convert::Infallible>(
//!             Restage::Descend(Ann { original: node.id() }),
//!         )
//!     },
//! )
//! .restage(&input)
//! .unwrap();
//!
//! assert_eq!(output.root().annotation().original, input.root().id());
//! ```
//!
//! # What the visitor answers
//!
//! For each node the visitor returns a [`Restage`] value:
//!
//! - [`Descend(annotation)`](Restage::Descend) — the driver stages a copy of this
//!   node over the results of its children, with the given annotation, and visits
//!   every child subtree. There is no variant that keeps a node without visiting
//!   its children: the only way a subtree goes unvisited is an explicit `Emit` for
//!   one of its ancestors.
//! - [`Emit(nodes)`](Restage::Emit) — the callback has already staged the
//!   replacement itself, through the [context](RestageContext)'s region operations
//!   or its raw [`builder()`](RestageContext::builder); these are the staged nodes
//!   that stand in for this node, and an empty vector drops it. The driver does not
//!   descend afterwards, so a callback that still wants part of the subtree
//!   restaged asks for it explicitly, for example with
//!   [`restage_children`](RestageContext::restage_children).
//!
//! Descent is structural and never depends on the role a child plays in its
//! parent: the driver visits children in
//! [`Attached`](crate::core::node::SlotRole::Attached) and
//! [`Hidden`](crate::core::node::SlotRole::Hidden) slot regions exactly like any
//! other child. Copying attached content unchanged is one explicit arm of your
//! visitor, not something the driver does on its own.
//!
//! # The input is readable, the output is not
//!
//! A callback reads the frozen input node with the full node API and the
//! [`extract`](crate::extract) helpers, and produces staged output it can only
//! refer to by [`BuildId`] — a staged node cannot be read back. In particular a
//! `Descend` parent never sees its children's results, since the decision is taken
//! before they are staged; whatever a callback stages it produced itself, and any
//! fact it needs later goes into its own closure state or into an annotation.
//!
//! A pass that has to inspect transformed output finishes the tree and runs a
//! second pass over it. Chaining passes is cheap: annotations can be replaced
//! without copying the tree
//! ([`annotate`](crate::core::node::NodeTree::annotate)), and node payloads are
//! shared through `Arc`.
//!
//! # Annotations
//!
//! Every output node's annotation comes from the visitor — as the value in
//! `Descend(annotation)`, or as an explicit argument to the staging operation the
//! callback invokes. The input and output annotation types are separate type
//! parameters, so "keep the input annotation" is deliberately not expressible.
//!
//! To record where an output node came from, store the input node's id in your own
//! annotation type: `Descend(Ann { original: node.id(), .. })`, as in the example
//! above. To find the new node made for an old id, invert the finished tree's
//! annotations once, in time proportional to the number of nodes:
//!
//! ```text
//! let new_of_old: HashMap<NodeId, NodeId> = output
//!     .iter_storage_order()
//!     .map(|n| (n.annotation().original, n.id()))
//!     .collect();
//! ```
//!
//! # Region edits are checked, never silently repaired
//!
//! The driver applies the same rules as construction during a parse: legal shapes
//! pass, and an edit that would leave a callable's records meaningless is an error.
//!
//! - Dropping every node of an argument's region restages the argument as
//!   **provided with an empty region**. An absent argument means something
//!   different from an empty one, and true absence is the explicit
//!   [`RestagedArgument::absent`].
//! - Dropping the node that a record's
//!   [`InChildrenOf`](crate::core::node::ContentNodes::InChildrenOf) content
//!   designation points into — or replacing it with several nodes — is
//!   [`RestageError::ContentParentDropped`]. Re-anchoring the designation
//!   somewhere else would change what the record means, and for a multi-node
//!   replacement there is no defined place to re-anchor it to. The remedy is to
//!   take the callable over: `Emit` its replacement yourself, staged with
//!   [`restage_invocation`](RestageContext::restage_invocation) or the raw builder.
//!
//! # Input nodes may come from any tree
//!
//! The context operations — and the
//! [`NodeTreeBuilder::restage_node`](crate::core::node::NodeTreeBuilder::restage_node)
//! primitive underneath them — accept nodes from **any** tree, not only from the
//! run's own input. This is supported by contract, so that one pass can assemble an
//! output tree out of pieces of several input trees.

use core::fmt;

use alloc::string::String;
use alloc::vec::Vec;

use crate::engine::DescentWarning;
use crate::node::{BuildId, NodeBuildError, NodeId, NodeRef};
use crate::state::Lang;

mod bundles;
mod context;

pub use bundles::{RestagedArgument, RestagedSlot};
pub use context::{RestageContext, TreeRestager};

/// What a [`RestageVisitor`] asks the driver to do with one input node.
///
/// Returned from [`RestageVisitor::restage`]; the [module docs](self) describe how
/// the driver acts on each variant.
#[derive(Clone, Debug)]
pub enum Restage<B> {
    /// Keep this node: the driver stages a copy of it over the results of its
    /// children, with the given annotation, and visits every child subtree.
    ///
    /// There is no variant that keeps a node without visiting its children.
    Descend(B),
    /// Replace this node with nodes the callback has already staged itself; an
    /// empty vector drops it.
    ///
    /// The driver does not visit the node's children afterwards.
    Emit(Vec<BuildId>),
}

/// The callback half of a [`TreeRestager`] run: implement this, and the driver
/// calls it once per input node.
///
/// Each call receives a node of the frozen input tree and the
/// [`RestageContext`] to stage output through, and answers a [`Restage`] value;
/// the [module docs](self) describe the contract and what the driver does between
/// two calls.
///
/// Any `FnMut(NodeRef<'_, L, A>, &mut RestageContext<'_, L, A, B>) ->
/// Result<Restage<B>, E>` closure is a visitor through the blanket
/// implementation, which is all a pass that never re-enters itself needs. This is
/// a trait rather than a plain closure parameter because the context's region
/// operations re-enter the visitor from *inside* a visitor call —
/// `cx.restage_argument(node, 0, self)` — and a closure cannot pass itself.
///
/// An inline closure must annotate both of its parameter types: `&mut |node:
/// NodeRef<'_, L, A>, cx: &mut RestageContext<'_, L, A, B>| { … }`, as spelled in
/// the [module](self) example. A fully unannotated `|node, cx|` does not infer
/// against the generic visitor parameter, while a function item needs no
/// annotations at all.
///
/// A visitor need not be `Send` or `Sync` (nor need an
/// [`annotate`](crate::core::node::NodeTree::annotate) callback): the driver runs
/// visitors synchronously on the calling thread, so such a bound would constrain
/// callers without buying anything, and it would exclude single-threaded
/// foreign-function callbacks.
///
/// # Errors
///
/// `Error` is the visitor's own failure type. It is returned unchanged from
/// [`TreeRestager::restage`], as [`RestageError::Visitor`]. Context operations
/// fail with [`RestageError`]`<Self::Error>` values instead, so a visitor that
/// wants to propagate those with `?` gives its own error type a conversion:
///
/// ```text
/// enum PassError {
///     Restage(Box<RestageError<PassError>>),  // op failures, boxed
///     BadReference(String),                   // the pass's own conditions
/// }
/// impl From<RestageError<PassError>> for PassError { … }
/// ```
pub trait RestageVisitor<L: Lang, A, B> {
    /// The visitor's own failure type (see the trait docs).
    type Error;

    /// Decides what becomes of `node` in the output tree.
    ///
    /// See [`Restage`] for the answers and the [module docs](self) for what the
    /// driver does with each of them.
    fn restage(
        &mut self,
        node: NodeRef<'_, L, A>,
        cx: &mut RestageContext<'_, L, A, B>,
    ) -> Result<Restage<B>, Self::Error>;

    /// Reports that the run's descent guard allowed a descent but is nearing its
    /// limit — under the unconfigured default, at half the stack budget (see
    /// [`StdDescentGuardInit`](crate::core::StdDescentGuardInit)).
    ///
    /// The warning is delivered here, rather than through the context, because
    /// state that spans a whole run belongs in the visitor's own `&mut self`
    /// fields. The default implementation ignores it.
    fn observe_descent_warning(&mut self, warning: DescentWarning) {
        let _ = warning;
    }
}

/// Every suitable `FnMut` closure is a [`RestageVisitor`], which covers any pass
/// that does not re-enter itself.
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

/// Why a [`TreeRestager`] run failed, generic over the visitor's own error type
/// `E`.
///
/// A failure of the visitor itself is returned unchanged as
/// [`Visitor`](RestageError::Visitor). The other variants report a violation of
/// the output builder's contract ([`Build`](RestageError::Build)), an edit the
/// driver cannot apply
/// ([`ContentParentDropped`](RestageError::ContentParentDropped)), a tree nested
/// deeper than the run's descent guard allows
/// ([`DescentLimitExceeded`](RestageError::DescentLimitExceeded)), or misuse of a
/// [`RestageContext`] operation — violating a documented contract returns an
/// `Err` here, it never panics.
///
/// `Clone`, `PartialEq`, and `Eq` are implemented when `E` implements them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RestageError<E> {
    /// Staging a node into the output builder failed because the staged input
    /// violated the builder's contract ([`NodeBuildError`]); the run is abandoned.
    Build(NodeBuildError),
    /// A restaged callable's argument or slot record designates its content
    /// inside a node ([`InChildrenOf`](crate::core::node::ContentNodes::InChildrenOf))
    /// that the pass dropped, or replaced with something other than exactly one
    /// node.
    ///
    /// There is no defined node to re-anchor the designation onto, so the driver
    /// refuses rather than silently change what the record means. Take the
    /// callable over instead: `Emit` its replacement, staged with
    /// [`restage_invocation`](RestageContext::restage_invocation) or the raw
    /// [`builder()`](RestageContext::builder).
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
    /// The root's replacement was not exactly one staged node.
    ///
    /// [`TreeRestager::restage`] returns a tree, and a wrapper node invented by
    /// the driver would need an annotation only the visitor can supply, so wrap
    /// the replacement in a node of your own and `Emit` that.
    RootNotSingular {
        /// How many staged nodes the visitor produced for the root.
        count: usize,
    },
    /// The input tree is nested more deeply than the run's descent guard allows,
    /// so the guard refused to go one level deeper and the run is abandoned.
    ///
    /// Configure the limit with [`TreeRestager::with_descent_guard_init`].
    DescentLimitExceeded {
        /// Which limit was hit and how to configure it.
        detail: String,
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
            RestageError::DescentLimitExceeded { detail } => {
                write!(f, "restage descent limit exceeded: {}", detail)
            }
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
