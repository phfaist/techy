//! Tree-to-value recomposition: combine a node tree into one value.
//!
//! The value can be anything that can be built by appending — a `String` of source
//! text or HTML, a vector of tokens, or nothing at all. The library attaches no
//! meaning to it; it only asks about each node and joins the answers together.
//!
//! The two items to know are a callback and a driver. You implement a
//! [`Recomposer`], which is asked about one node at a time and answers either
//! [`Emit(value)`](Recompose::Emit) — "the value for this node is exactly this" —
//! or [`Concat`](Recompose::Concat) — "combine my children's values", optionally
//! with a head, a separator, and a tail ([`ConcatPieces`]). You pass it to the
//! driver [`TreeRecomposer`]:
//! `TreeRecomposer::new(&mut recomposer).recompose(&tree, state)` asks about the
//! root, works downward, appends each node's children's values in document order to
//! make that node's value, and returns the value of the root.
//!
//! The value type is the recomposer's [`Piece`](Recomposer::Piece), which must
//! implement [`ComposePiece`]: an empty value plus an append operation. techy
//! implements it for `String` and for `()`.
//!
//! Re-emitting the original source text is one recomposer among others rather than
//! something the driver does by default; the ready-made one is the preset's
//! [`source_recomposer`](crate::latexlike::source_recomposer), and
//! [`core_source_instruction`] is the building block to write your own.
//!
//! This module is one of three tree consumers, which differ in what they produce:
//! [`visit`](crate::visit) produces nothing (a read-only traversal),
//! [`transform`](crate::transform) produces a new tree, and this one produces a
//! single value. [Node trees](crate::guide::node_trees) compares the three on one
//! page, and [Learn techy by example](crate::guide::learn_by_example) works one
//! transformation through from parse to re-emitted source.
//!
//! ```
//! use techy::core::{Language, ParsingState};
//! use techy::core::node::NodeRef;
//! use techy::error::Recovery;
//! use techy::latexlike::{Latexlike, LatexlikeDriver};
//! use techy::recompose::{
//!     core_source_instruction, Recompose, RecomposeContext, Recomposer, TreeRecomposer,
//! };
//!
//! // A minimal source re-emitter, for the node kinds the core can emit on its
//! // own. A real language preset also emits callables, from their recorded
//! // payload — see `latexlike::source_recomposer`.
//! struct CoreSource;
//!
//! impl<A> Recomposer<Latexlike, A> for CoreSource {
//!     type State = ();
//!     type Piece = String;
//!     type Error = String;
//!
//!     fn recompose_node(
//!         &mut self,
//!         node: NodeRef<'_, Latexlike, A>,
//!         _state: &(),
//!         _cx: &mut RecomposeContext<'_, Latexlike, A>,
//!     ) -> Result<Recompose<String, ()>, String> {
//!         core_source_instruction(node).ok_or_else(|| "a callable".to_string())
//!     }
//! }
//!
//! let language: Language<Latexlike> = Language::new(
//!     LatexlikeDriver::new(Recovery::Strict),
//!     ParsingState::lang_initial().expect("seed state"),
//! );
//! let tree = language.parse("a{b}c").unwrap().tree;
//! let out = TreeRecomposer::new(&mut CoreSource).recompose(&tree, ()).unwrap();
//! assert_eq!(out, "a{b}c");
//! ```
//!
//! # State is threaded downward
//!
//! A recomposer's [`State`](Recomposer::State) is context that flows from a node to
//! its children: a math nesting depth, a list level, an output mode. The entry
//! point gives the root its initial state, and a [`Concat`](Recompose::Concat) may
//! derive a different state for the children with
//! [`with_state`](ConcatPieces::with_state); children for which none is derived
//! inherit their parent's.
//!
//! State never flows back upward. Facts that must survive the whole run belong in
//! the recomposer's own `&mut self` fields, and accumulating the result is the
//! driver's job — the same division of state as in [`visit`](crate::visit).
//!
//! # Streaming output is the recomposer's business
//!
//! A run returns a value, and there is deliberately no writer or sink type in the
//! machinery. A recomposer that streams its output instead holds the writer in its
//! own `&mut self` fields and writes from
//! [`recompose_node`](Recomposer::recompose_node), which is called on the way down;
//! its `Piece` is then `()`, and appending `()` values costs nothing.
//!
//! # Which children `Concat` includes
//!
//! `Concat` combines the values of the node's plain children and of its `Content`
//! regions. Children in [`Attached`](crate::core::node::SlotRole::Attached) or
//! [`Hidden`](crate::core::node::SlotRole::Hidden) slot regions are left out unless
//! the instruction asks for them with
//! [`include_attached`](ConcatPieces::include_attached) or
//! [`include_hidden`](ConcatPieces::include_hidden).
//!
//! That default follows from what the two roles mean. An `Attached` region holds
//! *derived* content — the file an `\input` resolved to, say — which the invocation
//! that produced it already accounts for, and `Hidden` means the content
//! contributes neither output nor byte accounting. Recomposition is the one place
//! in the library where a slot role changes what happens: reading a tree, the
//! [walk](crate::visit::TreeWalker) included, visits every node the same way
//! regardless of role.
//!
//! # Wrapping one recomposer in another
//!
//! A run has exactly one recomposer, and every descent a `Concat` causes comes back
//! to it. A recomposer that *wraps* another one — overriding some nodes and
//! delegating the rest by calling the inner
//! [`recompose_node`](Recomposer::recompose_node) — therefore has its overrides
//! applied at every depth of the delegated subtrees, however many layers of
//! wrapping there are. A recomposer meant to be wrapped answers with instructions
//! and never descends on its own. (This is the opposite of
//! [`transform`](crate::transform), where a callback that takes a node over stages
//! its subtree itself.)
//!
//! To post-process what a `Concat` produced without breaking that property, attach
//! a function to the instruction with [`map`](ConcatPieces::map): the children are
//! still asked of the outermost recomposer, and the function runs afterwards on the
//! assembled result, head and tail included.
//!
//! The alternative is to combine the children inside
//! [`recompose_node`](Recomposer::recompose_node) yourself, through
//! [`recompose_children`](RecomposeContext::recompose_children) or another
//! [operation of the context](RecomposeContext). Those reach the children of any
//! node, but each of them asks the recomposer *you hand it*, so a recomposer that
//! passes `self` bypasses whatever wraps it for exactly those children. Do that
//! deliberately; otherwise post-process with `map`.
//!
//! Replacing the recomposition of selected nodes is this wrapping pattern, not a
//! mechanism of its own: wrap the base recomposer and override exactly those nodes.
//! Replacing the *content of the tree* is a different job — transform the tree with
//! [`TreeRestager`](crate::transform::TreeRestager) first, then recompose the
//! result.
//!
//! # A recomposer reads node payload, never source spans
//!
//! Each node is reconstructed from the data that node itself records:
//!
//! - *Permitted*: reading any field of the node's own payload, including resolving
//!   a span-backed payload field
//!   ([`TextContent::Spanned`](crate::source::TextContent)) against the node's own
//!   source. Whether such a field stores its text or a range into the source is an
//!   internal storage detail, and resolving it is what lets a freshly parsed tree
//!   be recomposed without copying any text.
//! - *Forbidden*: reading the source text under any **span**, the node's own span
//!   included, and any arithmetic between the spans of different nodes. On a
//!   transformed tree, the apparent gap between two siblings' spans would bring
//!   back content that was deleted. A span records where a node came from, not
//!   where its output goes.
//!
//! There is deliberately no faster path that reads the source directly for an
//! unmodified tree: a tree holds no reliable "still exactly as parsed" signal, so
//! such a shortcut could not be gated safely.
//! [`span_content()`](crate::core::node::NodeRef::span_content) stays available to
//! consumers generally — a recomposer simply does not use it.
//!
//! Byte-exact re-emission therefore rests entirely on the parse having recorded
//! everything: what a node stores is what recomposition can reproduce (see
//! [`CallableData::invocation_syntax`](crate::core::node::CallableData::invocation_syntax)).
//!
//! The same rule settles what a source recomposer promises for a language whose
//! [`OBEYS_SPAN_TILING`](crate::core::Lang::OBEYS_SPAN_TILING) is `false`: it
//! re-emits the tree **as stored**, including the text the parser recorded directly
//! where the content did not lie in the node's own source, and promises no byte
//! equality with any one source, since the tree's content need not be a range of
//! one. Nothing else changes; node data is read the same way in both cases.
//!
//! # Nesting depth is capped
//!
//! Recomposition recurses once per level of tree nesting, at a small and constant
//! stack cost per level, and every level passes the run's descent guard
//! ([`StdDescentGuard`](crate::core::StdDescentGuard), configured per run with
//! [`with_descent_guard_init`](TreeRecomposer::with_descent_guard_init)). A tree
//! nested more deeply than the configured limit — built by hand through
//! [`NodeTreeBuilder`](crate::core::node::NodeTreeBuilder), or recomposed on a
//! thread with a smaller stack than the parse ran on — is refused with
//! [`RecomposeError::DescentLimitExceeded`] instead of exhausting the thread's
//! stack. The guard's early warning, emitted under the unconfigured default at half
//! the stack budget, reaches the recomposer's
//! [`observe_descent_warning`](Recomposer::observe_descent_warning) method.

use core::fmt;

use alloc::boxed::Box;
use alloc::string::String;

use crate::engine::DescentWarning;
use crate::node::{NodeId, NodeKind, NodeRef};
use crate::state::Lang;

mod context;

pub use context::{RecomposeContext, TreeRecomposer};

/// A value type recomposition can build up: an empty value plus an append
/// operation.
///
/// This is the [`Piece`](Recomposer::Piece) of a [`Recomposer`] — the type of the
/// values it answers with and that the driver joins together. techy implements it
/// for `String`, for text output, and for `()`, for recomposers that write to a
/// writer of their own and return nothing. Your own value types — a vector of
/// tokens, a rope builder, a layout tree — implement it the same way.
///
/// `Clone` is a supertrait for exactly one reason: a
/// [`Concat`](Recompose::Concat) separator is appended once between every two
/// children, so the driver has to duplicate it.
pub trait ComposePiece: Clone {
    /// The empty value: appending it to anything, or anything to it, changes
    /// nothing.
    fn empty() -> Self;

    /// Appends `other` after `self`.
    ///
    /// This is deliberately infallible. Neither `String` nor `()` can fail here,
    /// and a recomposition already has a failure channel of its own
    /// ([`Recomposer::Error`]). If your own value type can fail while appending —
    /// one writing into an external buffer, say — record the failure somewhere the
    /// recomposer can see it, leave `self` unchanged, and answer
    /// [`Error`](Recomposer::Error) from the next call to
    /// [`recompose_node`](Recomposer::recompose_node).
    fn append(&mut self, other: Self);
}

impl ComposePiece for String {
    fn empty() -> String {
        String::new()
    }

    fn append(&mut self, other: String) {
        self.push_str(&other);
    }
}

/// The value type of a recomposer that produces no value: one that writes to a
/// writer of its own instead (see the [module docs](self)).
impl ComposePiece for () {
    fn empty() {}

    fn append(&mut self, _other: ()) {}
}

/// The callback half of a [`TreeRecomposer`] run: implement this, and the driver
/// asks it about one node at a time.
///
/// Each call receives the node, the [`State`](Recomposer::State) threaded down from
/// its parent, and the run's [`RecomposeContext`], and answers a [`Recompose`]
/// instruction: emit this value for the node, or combine the children's values.
/// The driver appends the children's values in document order to make the parent's;
/// see the [module docs](self).
///
/// Facts that span the whole run belong in the recomposer's own `&mut self` fields,
/// since `State` flows downward only. A recomposer need not be `Send` or `Sync`,
/// for the same reason a
/// [`RestageVisitor`](crate::transform::RestageVisitor) need not be: the driver
/// runs it synchronously on the calling thread.
pub trait Recomposer<L: Lang, A> {
    /// The context threaded down from a node to its children (module docs); `()`
    /// for a recomposer that needs none.
    type State;

    /// The type of value this recomposer produces (see [`ComposePiece`]).
    type Piece: ComposePiece;

    /// The recomposer's own failure type; it is returned unchanged from
    /// [`TreeRecomposer::recompose`], as [`RecomposeError::Recomposer`].
    type Error;

    /// Answers the [instruction](Recompose) for `node`, which is being recomposed
    /// under `state`.
    fn recompose_node(
        &mut self,
        node: NodeRef<'_, L, A>,
        state: &Self::State,
        cx: &mut RecomposeContext<'_, L, A>,
    ) -> Result<Recompose<Self::Piece, Self::State>, Self::Error>;

    /// Reports that the run's descent guard allowed a descent but is nearing its
    /// limit — under the unconfigured default, at half the stack budget (see
    /// [`StdDescentGuardInit`](crate::core::StdDescentGuardInit)).
    ///
    /// The warning is delivered here, rather than through the context, because
    /// state that spans a whole run belongs in the recomposer's own `&mut self`
    /// fields. The default implementation ignores it.
    fn observe_descent_warning(&mut self, warning: DescentWarning) {
        let _ = warning;
    }
}

/// What a [`Recomposer`] answers about one node, returned from
/// [`Recomposer::recompose_node`].
///
/// An instruction is not `Clone`, because a [`Concat`](Recompose::Concat) may hold
/// a post-processing function ([`ConcatPieces::map`]) that the one call carrying it
/// out consumes. That function is also why an instruction is neither `Send` nor
/// `Sync` for any value or state type: a post-processing closure is deliberately
/// not required to be either, and an instruction is built and consumed within a
/// single driver call, never passed to another thread.
#[derive(Debug)]
pub enum Recompose<P, S> {
    /// The value for this node is exactly this one.
    ///
    /// The driver does not descend: whatever the subtree ought to contribute is
    /// already part of the value.
    Emit(P),
    /// Recompose the node's children and append their values as
    /// `head + child₁ + sep + … + childₙ + tail` (see [`ConcatPieces`]).
    ///
    /// The children are recomposed under the parent's state, or under a state the
    /// instruction derives ([`with_state`](ConcatPieces::with_state)).
    Concat(ConcatPieces<P, S>),
}

/// How the children's values are joined for a [`Recompose::Concat`]:
/// `head + child₁ + sep + … + childₙ + tail`, plus the state to recompose them
/// under and which children to include.
///
/// Start from [`children()`](ConcatPieces::children) and chain the methods that
/// apply:
///
/// ```
/// # use techy::recompose::ConcatPieces;
/// // The children, wrapped in braces and comma-separated:
/// let instruction: ConcatPieces<String, ()> =
///     ConcatPieces::children().wrap("{", "}").join(", ");
/// ```
///
/// By default the children included are the node's plain children and its
/// `Content` regions; children in `Attached` or `Hidden` slot regions are left out
/// unless asked for (see the [module docs](self)).
///
/// A [`map`](ConcatPieces::map) function may be attached to post-process the
/// assembled result. It makes the instruction neither `Clone`, `Send`, nor `Sync`
/// (see [`Recompose`]).
pub struct ConcatPieces<P, S> {
    head: P,
    sep: P,
    tail: P,
    state: Option<S>,
    include_attached: bool,
    include_hidden: bool,
    map: Option<Box<dyn FnOnce(P) -> P>>,
}

impl<P: ComposePiece, S> ConcatPieces<P, S> {
    /// The starting point: append the children's values and nothing else — no
    /// head, separator, or tail, the parent's state inherited, and the default
    /// choice of children.
    pub fn children() -> ConcatPieces<P, S> {
        ConcatPieces {
            head: P::empty(),
            sep: P::empty(),
            tail: P::empty(),
            state: None,
            include_attached: false,
            include_hidden: false,
            map: None,
        }
    }

    /// Appends `head` before the first child and `tail` after the last.
    ///
    /// Both are emitted even when the node has no children in scope.
    pub fn wrap(mut self, head: impl Into<P>, tail: impl Into<P>) -> ConcatPieces<P, S> {
        self.head = head.into();
        self.tail = tail.into();
        self
    }

    /// Appends `sep` between every two consecutive children.
    ///
    /// It is cloned once per gap, which is why [`ComposePiece`] requires `Clone`.
    pub fn join(mut self, sep: impl Into<P>) -> ConcatPieces<P, S> {
        self.sep = sep.into();
        self
    }

    /// Recomposes the children under this state instead of the parent's (module
    /// docs).
    pub fn with_state(mut self, state: S) -> ConcatPieces<P, S> {
        self.state = Some(state);
        self
    }

    /// Also includes the children in
    /// [`Attached`](crate::core::node::SlotRole::Attached) slot regions, which are
    /// left out by default.
    pub fn include_attached(mut self) -> ConcatPieces<P, S> {
        self.include_attached = true;
        self
    }

    /// Also includes the children in
    /// [`Hidden`](crate::core::node::SlotRole::Hidden) slot regions, which are left
    /// out by default.
    pub fn include_hidden(mut self) -> ConcatPieces<P, S> {
        self.include_hidden = true;
        self
    }

    /// Post-processes this instruction's result: the driver applies `f` to the
    /// fully assembled value — `head + child₁ + sep + … + childₙ + tail`, head and
    /// tail included — and what `f` returns is what the node contributes to its
    /// parent.
    ///
    /// This is the way to post-process a result that keeps working when another
    /// recomposer wraps this one: the children are still recomposed by the
    /// outermost recomposer of the run (the [module docs](self) explain why), and
    /// `f` runs on their assembled result afterwards. A recomposer that instead
    /// recomposes the children itself — through an
    /// [operation of the context](RecomposeContext), passing `self` — bypasses any
    /// recomposer wrapping it for those children.
    ///
    /// ```
    /// # use techy::recompose::ConcatPieces;
    /// let instruction: ConcatPieces<String, ()> = ConcatPieces::children()
    ///     .wrap("<", ">")
    ///     .map(|piece| format!("[{piece}]"));
    /// ```
    ///
    /// Called twice, the functions run in the order they were attached: the first
    /// one runs first, and the second on its result.
    ///
    /// `f` runs only if recomposing this node succeeds. A failing child returns its
    /// error before the assembly completes, and the function is then dropped
    /// unused.
    ///
    /// `f` is deliberately infallible and `'static`, so it cannot borrow the
    /// recomposer: a recomposition already has a failure channel of its own
    /// ([`Recomposer::Error`]), so post-processing that can fail belongs in the
    /// recomposer, which records the failure and answers with its own error on the
    /// next call.
    pub fn map(mut self, f: impl FnOnce(P) -> P + 'static) -> ConcatPieces<P, S>
    where
        P: 'static,
    {
        self.map = Some(match self.map.take() {
            None => Box::new(f),
            Some(registered) => Box::new(move |piece| f(registered(piece))),
        });
        self
    }

    /// Take this instruction apart for the driver to carry out (see
    /// [`ConcatLowering`]).
    pub(crate) fn into_lowering(self) -> ConcatLowering<P, S> {
        ConcatLowering {
            head: self.head,
            sep: self.sep,
            tail: self.tail,
            state: self.state,
            include_attached: self.include_attached,
            include_hidden: self.include_hidden,
            map: self.map,
        }
    }
}

/// A [`ConcatPieces`] taken apart for the driver to carry out — the same fields,
/// owned by the driver instead of by the instruction.
pub(crate) struct ConcatLowering<P, S> {
    pub(crate) head: P,
    pub(crate) sep: P,
    pub(crate) tail: P,
    pub(crate) state: Option<S>,
    pub(crate) include_attached: bool,
    pub(crate) include_hidden: bool,
    pub(crate) map: Option<Box<dyn FnOnce(P) -> P>>,
}

impl<P: fmt::Debug, S: fmt::Debug> fmt::Debug for ConcatPieces<P, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// Stands in for the post-processing function, which has no rendering
        /// of its own: `map: Some(..)` reports that one is attached.
        struct Attached;
        impl fmt::Debug for Attached {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("..")
            }
        }

        f.debug_struct("ConcatPieces")
            .field("head", &self.head)
            .field("sep", &self.sep)
            .field("tail", &self.tail)
            .field("state", &self.state)
            .field("include_attached", &self.include_attached)
            .field("include_hidden", &self.include_hidden)
            .field("map", &self.map.as_ref().map(|_| Attached))
            .finish()
    }
}

/// Why a [`TreeRecomposer`] run failed, generic over the recomposer's own error
/// type `E`.
///
/// A failure of the recomposer itself is returned unchanged as
/// [`Recomposer`](RecomposeError::Recomposer). The other variants report a tree
/// nested deeper than the run's descent guard allows
/// ([`DescentLimitExceeded`](RecomposeError::DescentLimitExceeded)) or misuse of a
/// [`RecomposeContext`] operation — violating a documented contract returns an
/// `Err` here, it never panics. The variants match those of
/// [`RestageError`](crate::transform::RestageError) wherever the two modules can
/// fail the same way.
///
/// `Clone`, `PartialEq`, and `Eq` are implemented when `E` implements them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecomposeError<E> {
    /// The recomposer itself failed; its error, verbatim.
    Recomposer(E),
    /// [`recompose_argument_named`](RecomposeContext::recompose_argument_named)
    /// (or its `_content` sibling) was given a name that matches none of the
    /// callable's argument specs.
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
    /// An argument/slot/body op was applied to a node that is not a
    /// `Callable`.
    NotACallable {
        /// The offending node.
        node: NodeId,
    },
    /// [`recompose_slot_content_named`](RecomposeContext::recompose_slot_content_named)
    /// was given a name that matches none of the callable's slots.
    UnknownSlotName {
        /// The callable that was queried.
        node: NodeId,
        /// The unmatched slot name.
        name: String,
    },
    /// [`recompose_body`](RecomposeContext::recompose_body) was applied to a
    /// callable no slot of which is marked as its body
    /// ([`BodySlotExt::is_body`](crate::core::node::BodySlotExt::is_body)).
    NoBodySlot {
        /// The callable that was queried.
        node: NodeId,
    },
    /// The tree is nested more deeply than the run's descent guard allows, so the
    /// guard refused to go one level deeper and the run is abandoned.
    ///
    /// Configure the limit with [`TreeRecomposer::with_descent_guard_init`].
    DescentLimitExceeded {
        /// Which limit was hit and how to configure it.
        detail: String,
    },
}

impl<E: fmt::Display> fmt::Display for RecomposeError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecomposeError::Recomposer(error) => write!(f, "recomposer failed: {}", error),
            RecomposeError::UnknownArgumentName { node, name } => {
                write!(f, "callable {:?} has no argument named {:?}", node, name)
            }
            RecomposeError::ArgumentIndexOutOfRange { node, index, count } => write!(
                f,
                "callable {:?} has no argument #{} ({} arguments)",
                node, index, count
            ),
            RecomposeError::SlotIndexOutOfRange { node, index, count } => {
                write!(f, "callable {:?} has no slot #{} ({} slots)", node, index, count)
            }
            RecomposeError::NotACallable { node } => {
                write!(f, "node {:?} is not a callable", node)
            }
            RecomposeError::UnknownSlotName { node, name } => {
                write!(f, "callable {:?} has no slot named {:?}", node, name)
            }
            RecomposeError::NoBodySlot { node } => write!(
                f,
                "callable {:?} has no body slot (no slot ext reports is_body)",
                node
            ),
            RecomposeError::DescentLimitExceeded { detail } => {
                write!(f, "recompose descent limit exceeded: {}", detail)
            }
        }
    }
}

impl<E> core::error::Error for RecomposeError<E>
where
    E: core::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            RecomposeError::Recomposer(error) => Some(error),
            _ => None,
        }
    }
}

/// Returns the instruction that re-emits `node` as source text, for the node kinds
/// whose payload the core records completely.
///
/// This is the building block for writing a recomposer that reproduces a tree's
/// source spelling. The ready-made recomposer built on it is the preset's
/// [`source_recomposer`](crate::latexlike::source_recomposer); reach for this
/// function when you write a recomposer of your own — for another language, or one
/// that overrides some nodes and wants the ordinary spelling for all the rest.
///
/// - `Chars` — emits the content;
/// - `Comment` — emits the start delimiter, the comment text, and the whitespace
///   the comment consumed after it;
/// - `Group` — concatenates the children between the recorded delimiters;
/// - `List` — concatenates the children;
/// - `Callable` — returns `None`. How a callable is written is the language's
///   business and is recorded in
///   [`CallableData::invocation_syntax`](crate::core::node::CallableData::invocation_syntax),
///   so emitting one belongs to the language's own recomposer; the preset's
///   [`SourceRecomposer`](crate::latexlike::SourceRecomposer) reads that payload
///   at this point.
///
/// Every span-backed payload field is resolved against the node's own source, and
/// the source text under the node's span is never read — the reading rule of the
/// [module docs](self).
///
/// # Panics
///
/// Panics if a span-backed payload field of `node` names a range that is not within
/// the content of the node's own source, or does not fall on character boundaries
/// — the panic condition of
/// [`TextContent::resolve`](crate::source::TextContent::resolve). That means a tree
/// invariant is broken, which no parsed input can cause; only a tree assembled by
/// hand through [`NodeTreeBuilder`](crate::core::node::NodeTreeBuilder) can reach
/// it.
pub fn core_source_instruction<'t, L, A, P, S>(node: NodeRef<'t, L, A>) -> Option<Recompose<P, S>>
where
    L: Lang,
    P: ComposePiece + From<&'t str>,
{
    let source = node.span().source();
    match node.kind() {
        NodeKind::Chars { content } => Some(Recompose::Emit(P::from(content.resolve(source)))),
        NodeKind::Comment(data) => {
            let mut piece = P::from(data.start.resolve(source));
            piece.append(P::from(data.content.resolve(source)));
            piece.append(P::from(data.post_space.resolve(source)));
            Some(Recompose::Emit(piece))
        }
        NodeKind::Group(data) => Some(Recompose::Concat(
            ConcatPieces::children()
                .wrap(P::from(data.open.resolve(source)), P::from(data.close.resolve(source))),
        )),
        NodeKind::List => Some(Recompose::Concat(ConcatPieces::children())),
        NodeKind::Callable(_) => None,
    }
}

#[cfg(test)]
mod tests;
