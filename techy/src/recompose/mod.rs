//! Recomposition: a **meaning-free value fold** composing generic *pieces*
//! over the tree.
//!
//! The machinery attaches no meaning to what it composes: a [`Recomposer`]
//! answers one [instruction](Recompose) per node — *emit this piece* or
//! *concatenate the children's pieces* (with optional head/separator/tail,
//! [`ConcatPieces`]) — and the driver ([`recompose`]) folds the pieces
//! bottom-up into one value. Source re-emission is ONE recomposer
//! implementation (the latexlike preset's
//! [`SourceRecomposer`](crate::latexlike::SourceRecomposer)), never a
//! machinery default; the same fold renders plain text, HTML, token streams,
//! or nothing at all (`Piece = ()` — a scoped-state walk).
//!
//! ```
//! use techy::core::{Language, ParsingState};
//! use techy::core::node::NodeRef;
//! use techy::error::Recovery;
//! use techy::latexlike::{Latexlike, LatexlikeDriver};
//! use techy::recompose::{
//!     core_source_instruction, recompose, Recompose, RecomposeContext, Recomposer,
//! };
//!
//! // A minimal source reemitter over the core-complete kinds (a real language
//! // preset also handles callables from their recorded payload — see
//! // `latexlike::source_recomposer`).
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
//!     ParsingState::lang_initial(),
//! );
//! let tree = language.parse("a{b}c").unwrap().tree;
//! let out = recompose(&tree, (), &mut CoreSource).unwrap();
//! assert_eq!(out, "a{b}c");
//! ```
//!
//! # State: threaded downward by argument
//!
//! A recomposer's [`State`](Recomposer::State) is downward context (math depth,
//! list nesting, an output mode): the entry hands the root its initial state,
//! and a [`Concat`](Recompose::Concat) instruction optionally derives a new
//! state for the children ([`with_state`](ConcatPieces::with_state)) —
//! otherwise they inherit the parent's. State flows *down* only; run-spanning
//! facts live in the recomposer's own `&mut self` fields, and fold accumulation
//! is the driver's business (the three-channel discipline,
//! [`techy::visit`](crate::visit)).
//!
//! # No sink: streaming is a recomposer concern
//!
//! The fold returns a value; there is deliberately no sink type in the
//! machinery. A streaming recomposer holds its writer in `&mut self` and uses
//! `Piece = ()` — it writes on [`recompose_node`](Recomposer::recompose_node)
//! (enter order) and the `()` pieces compose for free.
//!
//! # The `Concat` scope: `Attached` and `Hidden` are skipped by default
//!
//! `Concat` folds the node's *plain children and `Content` regions*: children
//! in [`Attached`](crate::core::node::SlotRole::Attached) or
//! [`Hidden`](crate::core::node::SlotRole::Hidden) slot regions are skipped
//! unless explicitly opted in ([`include_attached`](ConcatPieces::include_attached) /
//! [`include_hidden`](ConcatPieces::include_hidden)). An `Attached` region is
//! *derived* content whose invocation text is its own recomposition (`\input`'s
//! resolved file), and `Hidden` means "no recomposition, no byte accounting" —
//! recompose is the **one role-sensitive site**; reads (the
//! [walk](crate::visit::walk) included) stay role-blind.
//!
//! # The wrapping contract: instructions lower against the outermost recomposer
//!
//! The driver holds exactly **one** recomposer, and every `Concat` descent
//! re-enters it — so a recomposer that *wraps* another (overriding some nodes,
//! delegating the rest with a plain inner `recompose_node` call) sees its
//! overrides applied at **every depth** of the delegated subtrees. A
//! wrap-intended recomposer therefore returns instructions and never descends
//! explicitly (contrast [`transform`](crate::transform), where a takeover
//! visitor stages its subtree itself). Layering is free: wrapping N deep still
//! lowers every instruction against the outermost value.
//!
//! **Targeted replacement is this pattern, not a mechanism**: to replace the
//! recomposition of selected nodes, wrap the base recomposer and override
//! exactly those nodes (there is no span-based fast path — see the reading
//! contract below). To replace the *tree content* itself, transform first and
//! reemit after: the restage→recompose pipeline
//! ([`transform::restage`](crate::transform::restage), then [`recompose`]).
//!
//! # The reading contract: payload only, spans are provenance
//!
//! A recomposer reconstructs each node from the node's **own recorded data**:
//!
//! - *Permitted*: reading any field of the node's own payload — including
//!   resolving span-backed payload
//!   ([`TextContent::Spanned`](crate::source::TextContent)) against the node's
//!   own source, an internal detail of how a content field is stored (parse
//!   trees recompose zero-copy).
//! - *Forbidden*: resolving any **span content** against the source — the
//!   node's own span included — and any **inter-node** span arithmetic
//!   ("apparent gaps" between siblings resurrect deleted content on any
//!   transformed tree). Spans give provenance, not output location.
//!
//! There is no span fast path: a tree carries no reliable "still fresh from
//! parse" signal, so a shortcut could never be safely gated.
//! [`span_content()`](crate::core::node::NodeRef::span_content) remains a
//! public *consumer* affordance — the recomposer simply never uses it.
//! Byte-exact reemission therefore rests entirely on payload completeness:
//! what the parse records is what recomposition can reproduce (the preset's
//! accuracy doctrine,
//! [`CallableData::invocation_syntax`](crate::core::node::CallableData::invocation_syntax)).

use core::fmt;

use alloc::string::String;

use crate::node::{NodeId, NodeKind, NodeRef};
use crate::state::Lang;

mod context;

pub use context::{recompose, RecomposeContext};

/// The piece monoid of a recomposition: an empty value and an associative
/// append — everything the fold needs to compose children's contributions.
///
/// techy implements it for `String` (text recomposition) and `()` (no
/// composed value — streaming recomposers hold their writer in `&mut self`
/// and compose unit pieces for free). Consumer piece types (token vectors,
/// rope builders, layout trees) implement it the same way.
///
/// The `Clone` supertrait exists for exactly one reason: a
/// [`Concat`](Recompose::Concat) separator is appended once **per gap**, so
/// the driver duplicates it.
pub trait ComposePiece: Clone {
    /// The empty piece (the fold's neutral element).
    fn empty() -> Self;

    /// Append `other` after `self`.
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

/// The no-value piece: streaming recomposers compose `()` and write to their
/// own writer instead (see the [module docs](self)).
impl ComposePiece for () {
    fn empty() {}

    fn append(&mut self, _other: ()) {}
}

/// A recomposition callback: invoked once per composed node with the node,
/// the downward [`State`](Recomposer::State), and the run's
/// [`RecomposeContext`]; answers the node's [instruction](Recompose).
///
/// Run-spanning state lives in the recomposer's own `&mut self` fields;
/// `State` threads *downward* only (module docs). Deliberately **no
/// `Send`/`Sync` bounds** (the
/// [`RestageVisitor`](crate::transform::RestageVisitor) argument: the fold
/// runs synchronously on the calling thread).
pub trait Recomposer<L: Lang, A> {
    /// The downward-threaded state (module docs); `()` for stateless
    /// recomposers.
    type State;

    /// The composed piece type (see [`ComposePiece`]).
    type Piece: ComposePiece;

    /// The recomposer's own failure type; it rides through [`recompose`]
    /// typed, as [`RecomposeError::Recomposer`].
    type Error;

    /// Answer the [instruction](Recompose) for `node` (composed under
    /// `state`).
    fn recompose_node(
        &mut self,
        node: NodeRef<'_, L, A>,
        state: &Self::State,
        cx: &mut RecomposeContext<'_, L, A>,
    ) -> Result<Recompose<Self::Piece, Self::State>, Self::Error>;
}

/// The recomposer's instruction for one node (returned from
/// [`Recomposer::recompose_node`]).
#[derive(Clone, Debug)]
pub enum Recompose<P, S> {
    /// This node's recomposition is exactly this piece; the driver does not
    /// descend (whatever the subtree should contribute is already in the
    /// piece).
    Emit(P),
    /// Fold the node's children and compose
    /// `head + child₁ + sep + … + childₙ + tail` ([`ConcatPieces`]). The
    /// children fold under the parent's state, or under the instruction's
    /// derived state ([`with_state`](ConcatPieces::with_state)).
    Concat(ConcatPieces<P, S>),
}

/// The joiner payload of [`Recompose::Concat`]:
/// `head + child₁ + sep + … + childₙ + tail`, plus the optional derived state
/// and the child scope. Built by chainable constructors:
///
/// ```
/// # use techy::recompose::ConcatPieces;
/// // The children, wrapped in braces and comma-separated:
/// let instruction: ConcatPieces<String, ()> =
///     ConcatPieces::children().wrap("{", "}").join(", ");
/// ```
///
/// The default scope is the node's **plain children and `Content` regions**:
/// children in `Attached` or `Hidden` slot regions are skipped unless
/// explicitly included (see the [module docs](self)).
#[derive(Clone, Debug)]
pub struct ConcatPieces<P, S> {
    head: P,
    sep: P,
    tail: P,
    state: Option<S>,
    include_attached: bool,
    include_hidden: bool,
}

impl<P: ComposePiece, S> ConcatPieces<P, S> {
    /// The seed: plain concatenation of the children's pieces (empty
    /// head/separator/tail, inherited state, default scope).
    pub fn children() -> ConcatPieces<P, S> {
        ConcatPieces {
            head: P::empty(),
            sep: P::empty(),
            tail: P::empty(),
            state: None,
            include_attached: false,
            include_hidden: false,
        }
    }

    /// Compose `head` before the first child and `tail` after the last
    /// (emitted even when there are no children).
    pub fn wrap(mut self, head: impl Into<P>, tail: impl Into<P>) -> ConcatPieces<P, S> {
        self.head = head.into();
        self.tail = tail.into();
        self
    }

    /// Compose `sep` between every two consecutive children (duplicated per
    /// gap — the [`ComposePiece`] `Clone` requirement).
    pub fn join(mut self, sep: impl Into<P>) -> ConcatPieces<P, S> {
        self.sep = sep.into();
        self
    }

    /// Fold the children under this derived state instead of inheriting the
    /// parent's (downward threading; module docs).
    pub fn with_state(mut self, state: S) -> ConcatPieces<P, S> {
        self.state = Some(state);
        self
    }

    /// Widen the scope to include children in
    /// [`Attached`](crate::core::node::SlotRole::Attached) slot regions.
    pub fn include_attached(mut self) -> ConcatPieces<P, S> {
        self.include_attached = true;
        self
    }

    /// Widen the scope to include children in
    /// [`Hidden`](crate::core::node::SlotRole::Hidden) slot regions.
    pub fn include_hidden(mut self) -> ConcatPieces<P, S> {
        self.include_hidden = true;
        self
    }

    /// Destructure for the driver: (head, sep, tail, derived state,
    /// include_attached, include_hidden).
    pub(crate) fn into_parts(self) -> (P, P, P, Option<S>, bool, bool) {
        (
            self.head,
            self.sep,
            self.tail,
            self.state,
            self.include_attached,
            self.include_hidden,
        )
    }
}

/// Error of a [`recompose`] run — generic over the recomposer's own error
/// type `E` (the framework's error rides through typed;
/// `Clone`/`PartialEq`/`Eq` are conditional on `E`, keeping the uniform-Clone
/// principle). The variant roster deliberately mirrors
/// [`RestageError`](crate::transform::RestageError) — the recompose surface
/// speaks the transform family's vocabulary; the variants beyond the
/// recomposer's own report misuse of a [context op](RecomposeContext) (a
/// documented-contract violation returns an `Err`, never panics — the panic
/// policy).
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
    /// callable none of whose slot exts designates a body
    /// ([`BodySlotExt::is_body`](crate::core::node::BodySlotExt::is_body)).
    NoBodySlot {
        /// The callable that was queried.
        node: NodeId,
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

/// The core-provided instruction for **source-faithful emission** of a node
/// from its own recorded payload — for the kinds whose payload the core owns
/// completely:
///
/// - `Chars` → emit the content;
/// - `Comment` → emit start delimiter + content + post-space;
/// - `Group` → concatenate the children wrapped in the recorded delimiters;
/// - `List` → concatenate the children;
/// - `Callable` → **`None`**: a callable's trigger spelling is Lang-owned
///   recorded payload
///   ([`CallableData::invocation_syntax`](crate::core::node::CallableData::invocation_syntax)),
///   so its emission belongs to the language's recomposer (the latexlike
///   preset's [`SourceRecomposer`](crate::latexlike::SourceRecomposer) reads
///   its payload enum here).
///
/// Per-node doctrine throughout: span-backed *payload* fields resolve against
/// the node's own source (permitted — a storage detail); the node's span
/// content is never consulted.
pub fn core_source_instruction<'t, L, A, P, S>(node: NodeRef<'t, L, A>) -> Option<Recompose<P, S>>
where
    L: Lang,
    P: ComposePiece + From<&'t str>,
{
    let source = node.span().source();
    match node.kind() {
        NodeKind::Chars { content } => Some(Recompose::Emit(P::from(content.resolve(source)))),
        NodeKind::Comment { content, start, post_space } => {
            let mut piece = P::from(start.resolve(source));
            piece.append(P::from(content.resolve(source)));
            piece.append(P::from(post_space.resolve(source)));
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
