//! Helpers that read content out of a parsed node list.
//!
//! These answer the everyday questions about a tree the parser already
//! produced: give me the text of this argument, split this list at commas, read
//! these `key=value` pairs. They are free functions over the node read API
//! rather than methods, which leaves [`core::node`](crate::core::node) a plain
//! storage-and-access layer that helpers can be added around.
//!
//! - [`content_as_chars`] — flattens a node sequence to a plain string.
//! - [`split_at_chars`] — splits a run of sibling nodes at a separator string,
//!   with grouped content protected; returns a [`SplitAtChars`].
//! - [`parse_keyval`] — reads `key1=value1,key2=value2,…` content; returns a
//!   [`KeyVals`].
//! - [`split_embellishments`] and [`split_tack_on_fields`] — read the two
//!   argument-content runs that the matching standard argument parsers produce,
//!   also as a [`KeyVals`].
//!
//! The guide puts these next to the other tree consumers, in [Extracting
//! content](crate::guide::node_trees#extracting-content-techyextract), and
//! works through them in [Learn techy by
//! example](crate::guide::learn_by_example#extracting-content).
//!
//! ```
//! use techy::core::{Language, ParsingState};
//! use techy::error::Recovery;
//! use techy::extract;
//! use techy::latexlike::{Latexlike, LatexlikeDriver};
//!
//! let language: Language<Latexlike> = Language::new(
//!     LatexlikeDriver::new(Recovery::Strict),
//!     ParsingState::lang_initial().expect("seed state"),
//! );
//! let result = language.parse("alpha,beta{x,y},gamma").unwrap();
//!
//! let split =
//!     extract::split_at_chars_drop_annotations(result.tree.root().children(), ",").unwrap();
//! assert_eq!(split.len(), 3);
//! // `source_text()` answers the segment's recorded coordinates — its content here,
//! // because this parse is span-tiled; `content_as_chars` reads the content itself
//! // and is the reader to use when a segment may be cut from owned content.
//! assert_eq!(split.segment(0).unwrap().source_text(), Some("alpha"));
//! // Grouped content protects its interior — and segments are ordinary node lists,
//! // so every helper composes:
//! assert_eq!(split.segment(1).unwrap().source_text(), Some("beta{x,y}"));
//! assert_eq!(extract::content_as_chars(split.segment(1).unwrap()).unwrap(), "betax,y");
//! ```
//!
//! # What the helpers take
//!
//! [`content_as_chars`] reads any node sequence
//! (`impl IntoIterator<Item = NodeRef>`): a [`NodeSlice`], an iterator, or a
//! node's children all work.
//!
//! The other helpers take a [`NodeSlice`], because they build a tree and need
//! the slice's own tree — its parsing state and its source — even when the
//! slice is empty, which a bare iterator cannot supply.
//!
//! # What the helpers return
//!
//! Splitting can cut *through* a chars node (`\cite{key1,my{x,y}z}`), and a
//! parsed tree is frozen, so the splitting helpers assemble a new [`NodeTree`]
//! and return it inside a [`SplitAtChars`] or a [`KeyVals`]. Nodes the split
//! did not touch are copied whole — spans, parsing states, and specs shared
//! through `Arc`, with fresh node ids — and a node a separator cut through
//! becomes a new `Chars` node holding the piece of that node's own content.
//!
//! A copied node keeps the language extension data it already had; the newly
//! created nodes — the cut pieces, the `List` wrappers, the root — get theirs from
//! [`Lang::make_node_ext`], as during a parse.
//!
//! Segments and values are then ordinary [`NodeSlice`] views into that result,
//! which is what makes the helpers compose with each other and with the rest of
//! the crate: a segment can be split again, flattened with
//! [`content_as_chars`], traversed with [`visit`](crate::visit), or rewritten
//! with [`transform`](crate::transform).
//!
//! Two things are worth knowing before reading spans off a result:
//!
//! - A piece cut out of a chars node whose content is **span-backed** records
//!   the exact sub-span of the original source. A piece cut out of a chars node
//!   whose content is **owned** — a materialized tree, the output of a
//!   transform pass, or a parse of a language with
//!   [`OBEYS_SPAN_TILING`](crate::core::Lang::OBEYS_SPAN_TILING) `= false` —
//!   keeps the whole original node's span instead, because owned text has no
//!   byte mapping into the source that could be subdivided.
//! - A result tree is a derived view: its sibling spans do not tile their
//!   parent's interior, since the separators are left out. It satisfies the law
//!   that every tree must satisfy
//!   ([`validate_tree`](crate::core::node::validate_tree)), but not the byte
//!   accounting of span tiling.
//!
//! Every helper reads a node's recorded **data** — chars content resolved
//! against the node's own source, group delimiters, callable names, argument
//! regions — and never the text a node's span points at. The answers below
//! therefore hold however a node stores its content, and the spans recorded on
//! a result are provenance coordinates and nothing more. Resolving span-backed
//! content is also the one way these helpers can panic: reading a tree that was
//! built by hand and records ranges that do not fit its source panics in
//! [`TextContent::resolve`](crate::source::TextContent::resolve). No parsed
//! input can produce such a tree.
//!
//! # Choosing the annotations of the result
//!
//! Every node of a tree holds a consumer-chosen annotation (the `A` of
//! `NodeTree<L, A>`). The four tree-building helpers accept input with any
//! annotation type and let the caller decide the output's, through a callback
//! invoked once for each node placed into the result. Each of the four
//! therefore comes in three spellings, the bare name being the general one:
//!
//! - [`split_at_chars(nodes, sep, f)`](split_at_chars) — `f` returns the
//!   annotation for one output node, given a part context
//!   ([`SplitAtCharsPart`] here, [`KeyValsPart`] for the other three helpers).
//!   The context answers what the callback cannot work out for itself: which
//!   input node this output node came from
//!   ([`original()`](SplitAtCharsPart::original) — `None` exactly for the
//!   synthesized `List` wrappers and the root), whether the output node is a
//!   piece cut out of that input node and what text was cut
//!   ([`is_partial()`](SplitAtCharsPart::is_partial),
//!   [`partial_text()`](SplitAtCharsPart::partial_text)), and which segment or
//!   entry the output node belongs to;
//! - [`split_at_chars_drop_annotations(nodes, sep)`](split_at_chars_drop_annotations)
//!   — every output node gets `()`;
//! - [`split_at_chars_keep_annotations(nodes, sep)`](split_at_chars_keep_annotations)
//!   — every output node keeps the annotation of the input node it came from
//!   (`A: Clone + Default`; the synthesized nodes get `A::default()`).
//!
//! [`parse_keyval`], [`split_embellishments`], and [`split_tack_on_fields`]
//! come in the same three spellings. The two suffixed forms are the general
//! form with a fixed callback, and they exist so that the `A: Clone + Default`
//! requirement falls on `_keep_annotations` alone, leaving the general form
//! free of any bound on the annotation type.
//!
//! The callback supplies annotations and nothing else. Dropping, replacing, or
//! rewriting nodes is what the restaging pass of
//! [`transform`](crate::transform) is for.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use crate::source::{Source, SourceSpan, Span, TextContent};
use crate::state::{Lang, ParsingState};

use crate::node::{
    copy_subtree_into, BuildId, NodeBuildError, NodeId, NodeKind, NodeRef, NodeSlice,
    NodeTree, NodeTreeBuilder,
};

// --- errors -----------------------------------------------------------------------------

/// The reason an extraction helper failed.
///
/// The helpers run over an already-parsed tree, outside any parse, so there is no
/// tolerant mode and nothing is reported as a diagnostic: input a helper cannot
/// handle is always an `Err`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtractError {
    /// A node that cannot flatten to characters was met — anything other than a
    /// chars node, a comment (skipped), or a group or list container (whose
    /// children are read instead). In practice this means a callable node,
    /// reached either by [`content_as_chars`] itself or by [`parse_keyval`]
    /// while flattening a key.
    NonCharsContent {
        /// The offending node, in the tree the input nodes came from.
        node: NodeId,
    },
    /// The separator passed to [`split_at_chars`] was the empty string, which
    /// has no occurrences to split at.
    EmptySeparator,
    /// [`split_embellishments`] or [`split_tack_on_fields`] met a node that is
    /// neither ignorable filler (a whitespace-only chars node, a comment) nor an
    /// entry of the shape it reads: the input was not the argument-content run
    /// that helper is documented for.
    UnexpectedContent {
        /// The offending node, in the tree the input nodes came from.
        node: NodeId,
    },
    /// Building the result tree failed. Every variant of [`NodeBuildError`] but
    /// one reports a violated builder contract, which is an implementation bug
    /// rather than anything about the input; the exception is
    /// [`ExtMintFailed`](NodeBuildError::ExtMintFailed), which reports that
    /// [`Lang::make_node_ext`] — called for each newly created node — failed.
    Build(NodeBuildError),
}

impl From<NodeBuildError> for ExtractError {
    fn from(error: NodeBuildError) -> ExtractError {
        ExtractError::Build(error)
    }
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtractError::NonCharsContent { node } => {
                write!(f, "content cannot flatten to characters: node {:?} is neither chars, comment, nor group/list", node)
            }
            ExtractError::EmptySeparator => write!(f, "split separator must not be empty"),
            ExtractError::UnexpectedContent { node } => {
                write!(f, "unexpected content in the input run: node {:?} is neither noise nor an entry of the expected shape", node)
            }
            ExtractError::Build(error) => write!(f, "building the result tree failed: {}", error),
        }
    }
}

impl core::error::Error for ExtractError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            ExtractError::Build(error) => Some(error),
            _ => None,
        }
    }
}

// --- pieces (internal currency between splitting and tree building) ---------------------

/// One piece of a split segment: a whole node, or — when a separator cut through a
/// chars node — a byte sub-range of a chars node's logical content. Internal; the
/// public types are trees and [`NodeSlice`]s.
struct Piece<'t, L: Lang, A> {
    node: NodeRef<'t, L, A>,
    /// `Some(sub)` = the sub-range `sub` of the node's chars content; `None` = whole
    /// node. Only ever `Some` for `Chars` nodes.
    part: Option<Range<usize>>,
}

impl<L: Lang, A> Clone for Piece<'_, L, A> {
    fn clone(&self) -> Self {
        Piece { node: self.node, part: self.part.clone() }
    }
}

fn whole<L: Lang, A>(node: NodeRef<'_, L, A>) -> Piece<'_, L, A> {
    Piece { node, part: None }
}

/// The piece's chars text, if it is a chars piece (whole chars node or partial).
fn piece_text<'t, L: Lang, A>(piece: &Piece<'t, L, A>) -> Option<&'t str> {
    let text = piece.node.chars()?;
    Some(match &piece.part {
        None => text,
        Some(sub) => text
            .get(sub.clone())
            .expect("piece sub-ranges lie on char boundaries of their node's content by construction"),
    })
}

/// The piece's provenance span: the node's span, narrowed to the sub-range for a
/// partial of span-backed content (exact); the whole node's span, unnarrowed, for a
/// partial of owned content — owned text has no byte mapping into the source to
/// subdivide (module docs).
fn piece_span<L: Lang, A>(piece: &Piece<'_, L, A>) -> SourceSpan<L::SourceOrigin> {
    if let Some(sub) = &piece.part {
        if let NodeKind::Chars { content: TextContent::Spanned(span), .. } = piece.node.kind() {
            return SourceSpan::new(
                piece.node.span().source(),
                span.start() + sub.start..span.start() + sub.end,
            );
        }
    }
    piece.node.span().clone()
}

// --- content_as_chars -------------------------------------------------------------------

/// Flattens a node sequence to its character content.
///
/// Chars nodes contribute their text, comments are skipped, and groups and lists
/// contribute their children recursively with the group delimiters left out.
/// Anything else — a callable — is an error.
///
/// This is how a string argument is read out of a call such as `\label{my-label}`
/// or `\href{https://…}{…}`, nested-group spellings like `\item[{*}]` included.
/// The input is any sequence of sibling nodes: an argument's content nodes
/// ([`NodeRef::argument_content_nodes`]), a [`NodeSlice`], a segment of a
/// [`SplitAtChars`], or a plain iterator.
///
/// An empty sequence gives `""`. Whitespace is returned as it stands — nothing is
/// trimmed. The result borrows its input when the flattened text is one contiguous
/// piece, which is the usual single-chars-node argument, and allocates only when
/// several pieces have to be joined.
///
/// To cut a run into parts before flattening it, see [`split_at_chars`]; for
/// `key=value` content, [`parse_keyval`].
///
/// This is pylatexenc's `get_content_as_chars()`.
///
/// # Errors
///
/// [`ExtractError::NonCharsContent`] if a callable node appears in the sequence, at
/// any depth: there is no text it could contribute.
///
/// # Panics
///
/// Panics on a broken tree invariant, as
/// [`NodeRef::chars`](crate::core::node::NodeRef::chars) does: a chars node's
/// span-backed content must be a valid `char`-boundary range of that node's own
/// source (module documentation).
pub fn content_as_chars<'t, L: Lang, A: 't>(
    nodes: impl IntoIterator<Item = NodeRef<'t, L, A>>,
) -> Result<Cow<'t, str>, ExtractError> {
    let mut acc = Acc::Empty;
    for node in nodes {
        collect_chars(node, None, &mut acc)?;
    }
    Ok(acc.finish())
}

/// `content_as_chars` over pieces (keyval keys).
fn pieces_as_chars<'t, L: Lang, A>(
    pieces: &[Piece<'t, L, A>],
) -> Result<Cow<'t, str>, ExtractError> {
    let mut acc = Acc::Empty;
    for piece in pieces {
        collect_chars(piece.node, piece.part.clone(), &mut acc)?;
    }
    Ok(acc.finish())
}

fn collect_chars<'t, L: Lang, A>(
    node: NodeRef<'t, L, A>,
    part: Option<Range<usize>>,
    acc: &mut Acc<'t>,
) -> Result<(), ExtractError> {
    match node.kind() {
        NodeKind::Chars { .. } => {
            let piece = Piece { node, part };
            acc.push(piece_text(&piece).expect("chars kind resolves text"));
            Ok(())
        }
        NodeKind::Comment { .. } => Ok(()),
        NodeKind::Group(_) | NodeKind::List => {
            for child in node.children() {
                collect_chars(child, None, acc)?;
            }
            Ok(())
        }
        NodeKind::Callable(_) => Err(ExtractError::NonCharsContent { node: node.id() }),
    }
}

/// Accumulator keeping the single-piece fast path allocation-free.
enum Acc<'t> {
    Empty,
    One(&'t str),
    Many(String),
}

impl<'t> Acc<'t> {
    fn push(&mut self, text: &'t str) {
        if text.is_empty() {
            return;
        }
        match self {
            Acc::Empty => *self = Acc::One(text),
            Acc::One(first) => {
                let mut owned = String::with_capacity(first.len() + text.len());
                owned.push_str(first);
                owned.push_str(text);
                *self = Acc::Many(owned);
            }
            Acc::Many(owned) => owned.push_str(text),
        }
    }

    fn finish(self) -> Cow<'t, str> {
        match self {
            Acc::Empty => Cow::Borrowed(""),
            Acc::One(text) => Cow::Borrowed(text),
            Acc::Many(owned) => Cow::Owned(owned),
        }
    }
}

// --- splitting --------------------------------------------------------------------------

/// Split pieces at occurrences of `sep` inside top-level chars pieces. Non-chars nodes
/// (groups, callables, comments) are never split and protect their interior —
/// pylatexenc's `split_at_chars` semantics. `max_split` caps the number of cuts;
/// `keep_empty` keeps empty segments (needed to tell `x=` from `x` in keyval).
fn split_pieces<'t, L: Lang, A>(
    input: impl IntoIterator<Item = Piece<'t, L, A>>,
    sep: &str,
    max_split: Option<usize>,
    keep_empty: bool,
) -> Vec<Vec<Piece<'t, L, A>>> {
    let mut segments: Vec<Vec<Piece<'t, L, A>>> = Vec::new();
    let mut current: Vec<Piece<'t, L, A>> = Vec::new();
    let mut splits = 0usize;

    fn flush<'t, L: Lang, A>(
        current: &mut Vec<Piece<'t, L, A>>,
        segments: &mut Vec<Vec<Piece<'t, L, A>>>,
        keep_empty: bool,
    ) {
        if keep_empty || !current.is_empty() {
            segments.push(core::mem::take(current));
        }
    }

    for piece in input {
        let splittable = max_split.is_none_or(|max| splits < max);
        let Some(text) = splittable.then(|| piece_text(&piece)).flatten() else {
            current.push(piece);
            continue;
        };
        let base = piece.part.as_ref().map_or(0, |sub| sub.start);
        let mut pos = 0;
        while max_split.is_none_or(|max| splits < max) {
            let Some(found) = text[pos..].find(sep) else { break };
            let at = pos + found;
            if at > pos {
                current.push(Piece { node: piece.node, part: Some(base + pos..base + at) });
            }
            flush(&mut current, &mut segments, keep_empty);
            splits += 1;
            pos = at + sep.len();
        }
        if pos == 0 {
            current.push(piece); // no cut in this piece — keep it untouched
        } else if pos < text.len() {
            current.push(Piece { node: piece.node, part: Some(base + pos..base + text.len()) });
        }
    }
    flush(&mut current, &mut segments, keep_empty);
    segments
}

// --- building result trees --------------------------------------------------------------

/// The anchor a builder helper synthesizes structure from: the input slice's covering
/// span and its first node's state — or, for an empty slice, an empty span at the
/// slice's tree root and the root's state.
fn anchor<L: Lang, A>(
    slice: &NodeSlice<'_, L, A>,
) -> (SourceSpan<L::SourceOrigin>, Arc<ParsingState<L>>) {
    match slice.first() {
        Some(first) => (
            slice.span().unwrap_or_else(|| first.span().clone()),
            first.parsing_state().clone(),
        ),
        None => {
            let root = slice.tree().root();
            let start = root.span().start();
            (
                SourceSpan::new(root.span().source(), start..start),
                root.parsing_state().clone(),
            )
        }
    }
}

/// The covering span of `first..last`, falling back to `first` alone across sources
/// (synthesized/mixed-origin trees).
fn covering<O: crate::source::SourceOrigin>(
    first: &SourceSpan<O>,
    last: &SourceSpan<O>,
) -> SourceSpan<O> {
    if first.same_source(last) && first.start() <= last.end() {
        SourceSpan::new(first.source(), first.start()..last.end())
    } else {
        first.clone()
    }
}

// --- part contexts (the annotation-mint callback's facts) -------------------------------

/// The facts shared by the per-helper part contexts; the public wrappers around it
/// stay separate opaque types, one per helper.
struct PartFacts<'t, L: Lang, A> {
    /// The input node this output node derives from; `None` for synthesized
    /// nodes (segment/value `List` wrappers and the root).
    original: Option<NodeRef<'t, L, A>>,
    /// The cut piece's text, when the output node is a boundary partial of a
    /// chars node.
    partial: Option<&'t str>,
    /// The segment (`split_at_chars`) or entry (keyval-shaped producers) the
    /// output node belongs to; `None` for the root.
    index: Option<usize>,
}

/// What [`split_at_chars`]'s annotation callback is told about one output node.
///
/// The callback is given one of these for every node placed into the result: each
/// copied node, each piece cut at a separator, each synthesized segment `List`, and
/// the root `List`. It reports only what the callback could not work out for itself
/// — see [`original`](Self::original), [`is_partial`](Self::is_partial),
/// [`partial_text`](Self::partial_text) and
/// [`segment_index`](Self::segment_index).
pub struct SplitAtCharsPart<'t, L: Lang, A = ()> {
    facts: PartFacts<'t, L, A>,
}

impl<'t, L: Lang, A> SplitAtCharsPart<'t, L, A> {
    /// The input node this output node came from — the node that was copied, or
    /// the chars node a piece was cut out of.
    ///
    /// `None` exactly for the nodes [`split_at_chars`] synthesizes: the one `List`
    /// per segment, and the root `List`.
    pub fn original(&self) -> Option<NodeRef<'t, L, A>> {
        self.facts.original
    }

    /// Whether the output node is a piece cut out of
    /// [`original`](Self::original) at a separator occurrence, rather than a copy
    /// of it.
    pub fn is_partial(&self) -> bool {
        self.facts.partial.is_some()
    }

    /// The text of the cut piece, or `None` when the output node is not one
    /// ([`is_partial`](Self::is_partial) answers the same question as a `bool`).
    pub fn partial_text(&self) -> Option<&'t str> {
        self.facts.partial
    }

    /// Which segment the output node belongs to, counting from `0` in source
    /// order; `None` exactly for the root `List`.
    pub fn segment_index(&self) -> Option<usize> {
        self.facts.index
    }
}

impl<L: Lang, A> fmt::Debug for SplitAtCharsPart<'_, L, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SplitAtCharsPart")
            .field("original", &self.facts.original.map(|node| node.id()))
            .field("partial", &self.facts.partial)
            .field("segment_index", &self.facts.index)
            .finish()
    }
}

/// What the annotation callback of a [`KeyVals`]-producing helper
/// ([`parse_keyval`], [`split_embellishments`], [`split_tack_on_fields`]) is told
/// about one output node.
///
/// The same facts as [`SplitAtCharsPart`], except that the index counts entries
/// rather than segments. Keys are plain strings rather than nodes, so no output
/// node ever comes from the key side of an entry.
pub struct KeyValsPart<'t, L: Lang, A = ()> {
    facts: PartFacts<'t, L, A>,
}

impl<'t, L: Lang, A> KeyValsPart<'t, L, A> {
    /// The input node this output node came from.
    ///
    /// `None` exactly for the synthesized nodes: the one `List` per entry value,
    /// and the root `List`.
    pub fn original(&self) -> Option<NodeRef<'t, L, A>> {
        self.facts.original
    }

    /// Whether the output node is a piece cut out of [`original`](Self::original)
    /// rather than a copy of it. Only [`parse_keyval`] cuts, at a `,` or the first
    /// `=`; [`split_embellishments`] and [`split_tack_on_fields`] always copy.
    pub fn is_partial(&self) -> bool {
        self.facts.partial.is_some()
    }

    /// The text of the cut piece, or `None` when the output node is not one.
    pub fn partial_text(&self) -> Option<&'t str> {
        self.facts.partial
    }

    /// Which entry the output node belongs to, counting from `0` in source order
    /// with duplicate keys counted separately; `None` exactly for the root
    /// `List`.
    pub fn entry_index(&self) -> Option<usize> {
        self.facts.index
    }
}

impl<L: Lang, A> fmt::Debug for KeyValsPart<'_, L, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyValsPart")
            .field("original", &self.facts.original.map(|node| node.id()))
            .field("partial", &self.facts.partial)
            .field("entry_index", &self.facts.index)
            .finish()
    }
}

/// The shape the helpers call internally: facts in, annotation out. The public
/// callbacks are adapted onto it through the per-helper wrapper types.
type Mint<'m, 't, L, A, B> = &'m mut dyn FnMut(PartFacts<'t, L, A>) -> B;

fn stage_piece<'t, L: Lang, A, B>(
    builder: &mut NodeTreeBuilder<L, B>,
    piece: &Piece<'t, L, A>,
    index: Option<usize>,
    mint: Mint<'_, 't, L, A, B>,
) -> Result<BuildId, NodeBuildError> {
    let Some(sub) = &piece.part else {
        return copy_subtree_into(builder, piece.node, &mut |original| {
            mint(PartFacts { original: Some(original), partial: None, index })
        });
    };
    let NodeKind::Chars { content, .. } = piece.node.kind() else {
        unreachable!("parts are only minted for Chars nodes (split_pieces)")
    };
    let (content, span) = match content {
        TextContent::Spanned(span) => (
            TextContent::Spanned(Span::new(span.start() + sub.start, span.start() + sub.end)),
            SourceSpan::new(
                piece.node.span().source(),
                span.start() + sub.start..span.start() + sub.end,
            ),
        ),
        // Owned content has no byte mapping into the source: owned sub-text, whole
        // original span as provenance (module docs).
        TextContent::Owned(text) => (
            TextContent::Owned(Box::from(
                text.get(sub.clone())
                    .expect("piece sub-ranges lie on char boundaries by construction"),
            )),
            piece.node.span().clone(),
        ),
    };
    // Boundary partials are fresh nodes: their ext is minted properly via
    // `make_node_ext` (the explicit transform-side recipe; copies keep their exts).
    let partial = piece_text(piece).expect("chars kind resolves text");
    let annotation =
        mint(PartFacts { original: Some(piece.node), partial: Some(partial), index });
    let kind = NodeKind::chars(content);
    let state = piece.node.parsing_state().clone();
    let ext = L::make_node_ext(&kind, &span, &state, builder.staged_children(&[]))?;
    builder.add(kind, span, state, Vec::new(), ext, annotation)
}

/// Stage one segment as a `List` node over its pieces. Empty segments (keyval's
/// explicitly-empty values) anchor at the fallback span/state.
fn stage_segment_list<'t, L: Lang, A, B>(
    builder: &mut NodeTreeBuilder<L, B>,
    pieces: &[Piece<'t, L, A>],
    fallback_span: &SourceSpan<L::SourceOrigin>,
    fallback_state: &Arc<ParsingState<L>>,
    index: usize,
    mint: Mint<'_, 't, L, A, B>,
) -> Result<BuildId, NodeBuildError> {
    let mut children = Vec::with_capacity(pieces.len());
    for piece in pieces {
        children.push(stage_piece(builder, piece, Some(index), mint)?);
    }
    let (span, state) = match (pieces.first(), pieces.last()) {
        (Some(first), Some(last)) => (
            covering(&piece_span(first), &piece_span(last)),
            first.node.parsing_state().clone(),
        ),
        _ => (fallback_span.clone(), fallback_state.clone()),
    };
    // Synthesized `List` wrappers are fresh nodes too: mint via `make_node_ext`;
    // their annotation comes from the callback with no original node.
    let annotation = mint(PartFacts { original: None, partial: None, index: Some(index) });
    let kind = NodeKind::list();
    let ext = L::make_node_ext(&kind, &span, &state, builder.staged_children(&children))?;
    builder.add(kind, span, state, children, ext, annotation)
}

// --- split_at_chars ---------------------------------------------------------------------

/// Splits a run of sibling nodes into segments at occurrences of `sep`.
///
/// Only the text of **top-level chars nodes** is searched for `sep`. Every other
/// node — a group, a callable, a comment — is kept whole and protects its interior:
/// splitting the argument of `\cite{key1,key2,my{special,key},keyN}` at `","` gives
/// four segments, the third being the chars piece `my` followed by the whole
/// `{special,key}` group.
///
/// Nothing is trimmed, so a separator surrounded by spaces leaves those spaces in
/// the neighboring segments. A run with no occurrence of `sep` at all gives one
/// segment holding the whole run, and an empty run gives no segments. Empty segments
/// are dropped, as pylatexenc does by default: `a,,b` gives two segments, `,a,` one,
/// and `,,` none.
///
/// The segments are stored in one new tree owned by the returned [`SplitAtChars`],
/// and read back as [`NodeSlice`]s through [`segment`](SplitAtChars::segment) and
/// [`segments`](SplitAtChars::segments). Being ordinary node lists, they feed every
/// other helper: [`content_as_chars`] flattens a segment, and `split_at_chars`
/// splits one again at a second separator.
///
/// `annotate` supplies the annotation of each output node from its
/// [`SplitAtCharsPart`] (module docs); it supplies annotations only, since changing
/// nodes is what [`transform`](crate::transform) is for. The input's own annotation
/// type is unconstrained. Two fixed choices ship as separate functions:
/// [`split_at_chars_drop_annotations`] (every output node gets `()`) and
/// [`split_at_chars_keep_annotations`] (each keeps its original node's annotation).
///
/// This is pylatexenc's `LatexNodeList.split_at_chars`.
///
/// # Examples
///
/// ```
/// use techy::core::{Language, ParsingState};
/// use techy::core::node::NodeId;
/// use techy::error::Recovery;
/// use techy::extract;
/// use techy::latexlike::{Latexlike, LatexlikeDriver};
///
/// let language: Language<Latexlike> = Language::new(
///     LatexlikeDriver::new(Recovery::Strict),
///     ParsingState::lang_initial().expect("seed state"),
/// );
/// let tree = language.parse("ab,c").unwrap().tree;
///
/// // Mint each output node's annotation: the original node's id, if any.
/// let split = extract::split_at_chars(tree.root().children(), ",", |part| {
///     part.original().map(|node| node.id())
/// })
/// .unwrap();
/// let ab = split.segment(0).unwrap().first().unwrap();
/// // The partial "ab" was cut out of the input's one chars node:
/// assert_eq!(*ab.annotation(), Some(tree.root().child(0).unwrap().id()));
/// // Synthesized wrappers derive from no input node:
/// assert_eq!(*split.tree().root().annotation(), None::<NodeId>);
/// ```
///
/// # Errors
///
/// [`ExtractError::EmptySeparator`] if `sep` is empty: the empty string has no
/// occurrences to split at.
///
/// [`ExtractError::Build`] if building the result tree fails.
///
/// # Panics
///
/// Panics on a broken tree invariant, as
/// [`NodeRef::chars`](crate::core::node::NodeRef::chars) does: a node's span-backed
/// content must be a valid `char`-boundary range of that node's own source (module
/// documentation).
pub fn split_at_chars<'t, L: Lang, A, B>(
    nodes: NodeSlice<'t, L, A>,
    sep: &str,
    mut annotate: impl FnMut(&SplitAtCharsPart<'t, L, A>) -> B,
) -> Result<SplitAtChars<L, B>, ExtractError> {
    let mut mint = |facts: PartFacts<'t, L, A>| annotate(&SplitAtCharsPart { facts });
    if sep.is_empty() {
        return Err(ExtractError::EmptySeparator);
    }
    let segments = split_pieces(nodes.iter().map(whole), sep, None, false);
    let (anchor_span, anchor_state) = anchor(&nodes);

    let mut builder = NodeTreeBuilder::new();
    let mut lists = Vec::with_capacity(segments.len());
    for (i, segment) in segments.iter().enumerate() {
        lists.push(stage_segment_list(
            &mut builder,
            segment,
            &anchor_span,
            &anchor_state,
            i,
            &mut mint,
        )?);
    }
    let span = match (segments.first().and_then(|s| s.first()), segments.last().and_then(|s| s.last()))
    {
        (Some(first), Some(last)) => covering(&piece_span(first), &piece_span(last)),
        _ => anchor_span,
    };
    let annotation = mint(PartFacts { original: None, partial: None, index: None });
    let kind = NodeKind::list();
    let ext = L::make_node_ext(&kind, &span, &anchor_state, builder.staged_children(&lists))?;
    let root = builder.add(kind, span, anchor_state, lists, ext, annotation)?;
    Ok(SplitAtChars { tree: builder.finish(root)? })
}

/// [`split_at_chars`] with `()` as every output node's annotation.
///
/// The form to reach for when the result's annotations are of no interest — which
/// is most of the time.
pub fn split_at_chars_drop_annotations<L: Lang, A>(
    nodes: NodeSlice<'_, L, A>,
    sep: &str,
) -> Result<SplitAtChars<L>, ExtractError> {
    split_at_chars(nodes, sep, |_| ())
}

/// [`split_at_chars`] keeping the input's annotations.
///
/// Each output node takes the annotation of the input node it came from; the
/// segment `List`s and the root, which come from no input node, get
/// `A::default()`. This is the only form of the three that requires anything of the
/// annotation type.
pub fn split_at_chars_keep_annotations<L: Lang, A: Clone + Default>(
    nodes: NodeSlice<'_, L, A>,
    sep: &str,
) -> Result<SplitAtChars<L, A>, ExtractError> {
    split_at_chars(nodes, sep, keep_annotation)
}

/// The callback `_keep_annotations` supplies: clone the original node's annotation,
/// or use the default for a synthesized node.
fn keep_annotation<'t, L: Lang, A: Clone + Default>(part: &SplitAtCharsPart<'t, L, A>) -> A {
    part.original().map(|node| node.annotation().clone()).unwrap_or_default()
}

/// The segments produced by [`split_at_chars`].
///
/// The segments are stored in one owned [`NodeTree`] — a root `List` with one `List`
/// child per segment — whose annotation type `B` is whatever the `annotate` callback
/// returned. Read the segments with [`segment`](Self::segment) and
/// [`segments`](Self::segments); they are ordinary [`NodeSlice`]s and can be fed
/// back to any helper of this module.
pub struct SplitAtChars<L: Lang, B = ()> {
    tree: NodeTree<L, B>,
}

impl<L: Lang, B> SplitAtChars<L, B> {
    /// Returns the number of segments.
    pub fn len(&self) -> usize {
        self.tree.root().child_count()
    }

    /// Returns whether there are no segments, which happens when the input run was
    /// empty or consisted only of separators.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the nodes of segment `i`, counting from `0` in source order, or
    /// `None` if there is no such segment.
    pub fn segment(&self, i: usize) -> Option<NodeSlice<'_, L, B>> {
        Some(self.tree.root().child(i)?.children())
    }

    /// Returns the segments in source order.
    pub fn segments(&self) -> impl Iterator<Item = NodeSlice<'_, L, B>> {
        self.tree.root().children().iter().map(|list| list.children())
    }

    /// Returns the tree the segments are stored in: a root `List` with one `List`
    /// child per segment.
    ///
    /// It is a derived tree, so its sibling spans do not tile their parent's
    /// interior — the separators are left out of it (module docs).
    pub fn tree(&self) -> &NodeTree<L, B> {
        &self.tree
    }

    /// Consumes this value and returns the tree the segments are stored in (see
    /// [`tree`](Self::tree)).
    pub fn into_tree(self) -> NodeTree<L, B> {
        self.tree
    }
}

impl<L: Lang, B> fmt::Debug for SplitAtChars<L, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SplitAtChars").field("segments", &self.len()).finish()
    }
}

// --- parse_keyval -----------------------------------------------------------------------

/// Reads a run of sibling nodes as `key1=value1,key2=value2,…` content.
///
/// Pairs are separated by top-level `,`, and within a pair the first top-level `=`
/// separates the key from the value; a later `=` stays in the value. Grouped content
/// protects both, so `legend={a,b}` is one pair whose value is the group.
///
/// Keys are flattened with [`content_as_chars`] and whitespace-trimmed. Values are
/// recorded raw — not trimmed, in source order, duplicate keys preserved.
///
/// The two ways a key can come without a value stay distinct, where pylatexenc
/// conflates them: `key=` records an empty value (an empty [`NodeSlice`]), and a
/// bare `key` with no `=` records no value at all ([`KeyValEntry::value`] is
/// `None`).
///
/// Empty pairs are dropped, so `a,,b` reads as two entries and a trailing comma adds
/// none; an empty run reads as no entries. A pair holding only whitespace is still an
/// entry, with the empty string as its key.
///
/// Duplicate keys are kept on purpose, and there are no aggregation settings:
/// [`get`](KeyVals::get) answers the everyday "which value is in effect" question
/// with the last occurrence, matching LaTeX's override behavior;
/// [`get_combined_with`](KeyVals::get_combined_with) joins them all; and
/// [`iter`](KeyVals::iter) gives them in source order, which is all pylatexenc's
/// `repeated_key_aggregate_action` policies need to be written as one-liners.
///
/// Trimming keys is a deliberate difference from pylatexenc, which keeps `" a "` as
/// written: LaTeX's keyval packages trim, and untrimmed keys are a recurring source
/// of mistakes.
///
/// `annotate` supplies the annotation of each output node from its [`KeyValsPart`]
/// (module docs); the fixed forms are [`parse_keyval_drop_annotations`] and
/// [`parse_keyval_keep_annotations`].
///
/// This is pylatexenc's `parse_keyval_content`.
///
/// # Errors
///
/// [`ExtractError::NonCharsContent`] if a key does not flatten to characters — a
/// callable standing in the key position, say.
///
/// [`ExtractError::Build`] if building the result tree fails.
///
/// # Panics
///
/// Panics on a broken tree invariant, as
/// [`NodeRef::chars`](crate::core::node::NodeRef::chars) does: a node's span-backed
/// content must be a valid `char`-boundary range of that node's own source (module
/// documentation).
pub fn parse_keyval<'t, L: Lang, A, B>(
    nodes: NodeSlice<'t, L, A>,
    mut annotate: impl FnMut(&KeyValsPart<'t, L, A>) -> B,
) -> Result<KeyVals<L, B>, ExtractError> {
    let mut mint = |facts: PartFacts<'t, L, A>| annotate(&KeyValsPart { facts });
    let (anchor_span, anchor_state) = anchor(&nodes);
    let comma_parts = split_pieces(nodes.iter().map(whole), ",", None, false);

    let mut builder = NodeTreeBuilder::new();
    let mut value_lists: Vec<BuildId> = Vec::new();
    let mut entries: Vec<(Box<str>, Option<usize>)> = Vec::new();
    for part in &comma_parts {
        let mut eq_parts = split_pieces(part.iter().cloned(), "=", Some(1), true);
        debug_assert!((1..=2).contains(&eq_parts.len()), "max_split=1 on nonempty input");
        let value_pieces = if eq_parts.len() == 2 { eq_parts.pop() } else { None };
        let key = pieces_as_chars(&eq_parts[0])?;
        let value = match value_pieces {
            Some(pieces) => {
                value_lists.push(stage_segment_list(
                    &mut builder,
                    &pieces,
                    &anchor_span,
                    &anchor_state,
                    entries.len(),
                    &mut mint,
                )?);
                Some(value_lists.len() - 1)
            }
            None => None,
        };
        entries.push((key.trim().into(), value));
    }

    finish_keyvals(builder, value_lists, entries, anchor_span, anchor_state, &mut mint)
}

/// [`parse_keyval`] with `()` as every output node's annotation.
pub fn parse_keyval_drop_annotations<L: Lang, A>(
    nodes: NodeSlice<'_, L, A>,
) -> Result<KeyVals<L>, ExtractError> {
    parse_keyval(nodes, |_| ())
}

/// [`parse_keyval`] keeping the input's annotations: each output node takes the
/// annotation of the input node it came from, and the synthesized nodes get
/// `A::default()`.
pub fn parse_keyval_keep_annotations<L: Lang, A: Clone + Default>(
    nodes: NodeSlice<'_, L, A>,
) -> Result<KeyVals<L, A>, ExtractError> {
    parse_keyval(nodes, keep_keyval_annotation)
}

/// The callback the keyval-family `_keep_annotations` forms supply.
fn keep_keyval_annotation<'t, L: Lang, A: Clone + Default>(part: &KeyValsPart<'t, L, A>) -> A {
    part.original().map(|node| node.annotation().clone()).unwrap_or_default()
}

/// Assembles a [`KeyVals`] from the staged value lists and a `(key, value-list index)`
/// table — the shared tail of [`parse_keyval`], [`split_embellishments`], and
/// [`split_tack_on_fields`].
fn finish_keyvals<'t, L: Lang, A, B>(
    mut builder: NodeTreeBuilder<L, B>,
    value_lists: Vec<BuildId>,
    entries: Vec<(Box<str>, Option<usize>)>,
    anchor_span: SourceSpan<L::SourceOrigin>,
    anchor_state: Arc<ParsingState<L>>,
    mint: Mint<'_, 't, L, A, B>,
) -> Result<KeyVals<L, B>, ExtractError> {
    let annotation = mint(PartFacts { original: None, partial: None, index: None });
    let kind = NodeKind::list();
    let ext =
        L::make_node_ext(&kind, &anchor_span, &anchor_state, builder.staged_children(&value_lists))?;
    let root = builder.add(kind, anchor_span, anchor_state, value_lists, ext, annotation)?;
    let tree = builder.finish(root)?;
    // The root's children are the staged value lists, in staging order.
    let value_ids: Vec<NodeId> = tree.root().children().iter().map(|list| list.id()).collect();
    let entries = entries
        .into_iter()
        .map(|(key, value)| KeyValEntryData { key, value: value.map(|i| value_ids[i]) })
        .collect();
    Ok(KeyVals { tree, entries })
}

// --- split_embellishments / split_tack_on_fields ----------------------------------------

/// Whether `node` is ignorable filler in a run — a comment, or a whitespace-only
/// chars node, which is how whitespace before an entry is stored in an argument
/// region.
fn is_run_noise<L: Lang, A>(node: &NodeRef<'_, L, A>) -> bool {
    match node.kind() {
        NodeKind::Comment { .. } => true,
        NodeKind::Chars { .. } => node
            .chars()
            .is_some_and(|text| text.chars().all(char::is_whitespace)),
        _ => false,
    }
}

/// Reads the content run of an embellishments argument as [`KeyVals`].
///
/// Give it the *content* nodes of an argument parsed by
/// [`EmbellishmentsArgumentParser`](crate::core::constructs::EmbellishmentsArgumentParser)
/// — [`NodeRef::argument_content_nodes`] returns them. Each embellishment becomes
/// one entry, in source order: the key is the marker that introduced it, which is
/// the opening delimiter of its wrapper group (`"^"`, `"_"`, …), and the value is
/// the embellishment's own nodes with filler removed, so the whitespace node that a
/// `^ {a}`-style pair stores inside its wrapper does not appear.
///
/// Whitespace and comments between embellishments are skipped. An empty run, or one
/// holding nothing but those, reads as no entries. Every entry has a value, possibly
/// an empty one — unlike [`split_tack_on_fields`], this helper never records an
/// entry without a value.
///
/// [`KeyVals`] fits embellishments closely: duplicate markers stay in source order,
/// [`get`](KeyVals::get) answers with the last occurrence, and
/// [`value_content`](KeyValEntry::value_content) unwraps the usual lone `{…}` value
/// group, turning `^{ab}` into the `ab` content.
///
/// `annotate` supplies the annotation of each output node from its [`KeyValsPart`]
/// (module docs); the fixed forms are [`split_embellishments_drop_annotations`] and
/// [`split_embellishments_keep_annotations`].
///
/// # Errors
///
/// [`ExtractError::UnexpectedContent`] if the run holds a node that is neither
/// whitespace, a comment, nor a group — the sign that the input was not an
/// embellishments argument's content run.
///
/// [`ExtractError::Build`] if building the result tree fails.
///
/// # Panics
///
/// Panics on a broken tree invariant, as
/// [`NodeRef::chars`](crate::core::node::NodeRef::chars) does: a node's span-backed
/// content must be a valid `char`-boundary range of that node's own source (module
/// documentation).
pub fn split_embellishments<'t, L: Lang, A, B>(
    nodes: NodeSlice<'t, L, A>,
    mut annotate: impl FnMut(&KeyValsPart<'t, L, A>) -> B,
) -> Result<KeyVals<L, B>, ExtractError> {
    let mut mint = |facts: PartFacts<'t, L, A>| annotate(&KeyValsPart { facts });
    let (anchor_span, anchor_state) = anchor(&nodes);
    let mut builder = NodeTreeBuilder::new();
    let mut value_lists: Vec<BuildId> = Vec::new();
    let mut entries: Vec<(Box<str>, Option<usize>)> = Vec::new();
    for node in nodes.iter() {
        if is_run_noise(&node) {
            continue;
        }
        let Some((marker, _close)) = node.group_delimiters() else {
            return Err(ExtractError::UnexpectedContent { node: node.id() });
        };
        let pieces: Vec<Piece<'_, L, A>> = node
            .children()
            .iter()
            .filter(|child| !is_run_noise(child))
            .map(whole)
            .collect();
        value_lists.push(stage_segment_list(
            &mut builder,
            &pieces,
            &anchor_span,
            &anchor_state,
            entries.len(),
            &mut mint,
        )?);
        entries.push((Box::from(marker), Some(value_lists.len() - 1)));
    }
    finish_keyvals(builder, value_lists, entries, anchor_span, anchor_state, &mut mint)
}

/// [`split_embellishments`] with `()` as every output node's annotation.
pub fn split_embellishments_drop_annotations<L: Lang, A>(
    nodes: NodeSlice<'_, L, A>,
) -> Result<KeyVals<L>, ExtractError> {
    split_embellishments(nodes, |_| ())
}

/// [`split_embellishments`] keeping the input's annotations: each output node takes
/// the annotation of the input node it came from, and the synthesized nodes get
/// `A::default()`.
pub fn split_embellishments_keep_annotations<L: Lang, A: Clone + Default>(
    nodes: NodeSlice<'_, L, A>,
) -> Result<KeyVals<L, A>, ExtractError> {
    split_embellishments(nodes, keep_keyval_annotation)
}

/// Reads the content run of a tack-on fields argument as [`KeyVals`].
///
/// Give it the *content* nodes of an argument parsed by
/// [`TackOnFieldsArgumentParser`](crate::core::constructs::TackOnFieldsArgumentParser)
/// — [`NodeRef::argument_content_nodes`] returns them. Each absorbed field invocation
/// becomes one entry, in source order: the key is the field's command name
/// (`"label"`), and the value is the content nodes of every argument the invocation
/// actually supplied, concatenated in argument order. For the common one-argument
/// field that is exactly that argument's content.
///
/// An invocation that supplied no argument at all — a flag field taking none, or one
/// whose arguments were all absent — records no value ([`KeyValEntry::value`] is
/// `None`), which keeps it distinct from an explicitly empty `\label{}`, whose value
/// is an empty slice. It is the same distinction [`parse_keyval`] draws between
/// `draft` and `label=`.
///
/// Whitespace and comments between fields are skipped. An empty run, or one holding
/// nothing but those, reads as no entries. Repeated fields — several `\label`s, say —
/// stay in source order: [`get`](KeyVals::get) answers with the last, and
/// [`get_combined_with`](KeyVals::get_combined_with) joins them all.
///
/// `annotate` supplies the annotation of each output node from its [`KeyValsPart`]
/// (module docs); the fixed forms are [`split_tack_on_fields_drop_annotations`] and
/// [`split_tack_on_fields_keep_annotations`].
///
/// # Errors
///
/// [`ExtractError::UnexpectedContent`] if the run holds a node that is neither
/// whitespace, a comment, nor a callable invocation with arguments — the sign that
/// the input was not a tack-on fields argument's content run.
///
/// [`ExtractError::Build`] if building the result tree fails.
///
/// # Panics
///
/// Panics on a broken tree invariant, as
/// [`NodeRef::chars`](crate::core::node::NodeRef::chars) does: a node's span-backed
/// content must be a valid `char`-boundary range of that node's own source (module
/// documentation).
pub fn split_tack_on_fields<'t, L: Lang, A, B>(
    nodes: NodeSlice<'t, L, A>,
    mut annotate: impl FnMut(&KeyValsPart<'t, L, A>) -> B,
) -> Result<KeyVals<L, B>, ExtractError> {
    let mut mint = |facts: PartFacts<'t, L, A>| annotate(&KeyValsPart { facts });
    let (anchor_span, anchor_state) = anchor(&nodes);
    let mut builder = NodeTreeBuilder::new();
    let mut value_lists: Vec<BuildId> = Vec::new();
    let mut entries: Vec<(Box<str>, Option<usize>)> = Vec::new();
    for node in nodes.iter() {
        if is_run_noise(&node) {
            continue;
        }
        let (Some(name), Some(arguments)) = (node.name(), node.arguments()) else {
            return Err(ExtractError::UnexpectedContent { node: node.id() });
        };
        let mut pieces: Option<Vec<Piece<'_, L, A>>> = None;
        for i in 0..arguments.len() {
            if let Some(content) = node.argument_content_nodes(i) {
                pieces.get_or_insert_with(Vec::new).extend(content.iter().map(whole));
            }
        }
        let value = match pieces {
            Some(pieces) => {
                value_lists.push(stage_segment_list(
                    &mut builder,
                    &pieces,
                    &anchor_span,
                    &anchor_state,
                    entries.len(),
                    &mut mint,
                )?);
                Some(value_lists.len() - 1)
            }
            None => None,
        };
        entries.push((Box::from(name), value));
    }
    finish_keyvals(builder, value_lists, entries, anchor_span, anchor_state, &mut mint)
}

/// [`split_tack_on_fields`] with `()` as every output node's annotation.
pub fn split_tack_on_fields_drop_annotations<L: Lang, A>(
    nodes: NodeSlice<'_, L, A>,
) -> Result<KeyVals<L>, ExtractError> {
    split_tack_on_fields(nodes, |_| ())
}

/// [`split_tack_on_fields`] keeping the input's annotations: each output node takes
/// the annotation of the input node it came from, and the synthesized nodes get
/// `A::default()`.
pub fn split_tack_on_fields_keep_annotations<L: Lang, A: Clone + Default>(
    nodes: NodeSlice<'_, L, A>,
) -> Result<KeyVals<L, A>, ExtractError> {
    split_tack_on_fields(nodes, keep_keyval_annotation)
}

struct KeyValEntryData {
    key: Box<str>,
    /// The value `List` node in the backing tree; `None` when the entry has no
    /// value.
    value: Option<NodeId>,
}

/// A list of key/value entries, as produced by [`parse_keyval`],
/// [`split_embellishments`], or [`split_tack_on_fields`].
///
/// The entries are in source order with duplicate keys preserved, reachable by
/// position with [`keyval`](Self::keyval), in order with [`iter`](Self::iter), and by
/// name with [`get`](Self::get), which answers with the last entry of that name.
///
/// The values are [`NodeSlice`]s into one owned [`NodeTree`] held here, whose
/// annotation type `B` is whatever the `annotate` callback returned. A lookup by name
/// scans the entries, which are few in the lists these helpers read.
pub struct KeyVals<L: Lang, B = ()> {
    tree: NodeTree<L, B>,
    entries: Vec<KeyValEntryData>,
}

impl<L: Lang, B> KeyVals<L, B> {
    /// Returns the number of entries, counting duplicate keys separately.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether there are no entries at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns entry `i`, counting from `0` in source order, or `None` if there is
    /// no such entry.
    pub fn keyval(&self, i: usize) -> Option<KeyValEntry<'_, L, B>> {
        self.entries.get(i).map(|entry| self.view(entry))
    }

    /// Returns the **last** entry named `key`, or `None` if there is none.
    ///
    /// The last one is the value in effect under LaTeX's keyval override behavior.
    /// Earlier occurrences of the same key are still reachable through
    /// [`iter`](Self::iter), and [`get_combined_with`](Self::get_combined_with) joins
    /// them all into one node run.
    pub fn get(&self, key: &str) -> Option<KeyValEntry<'_, L, B>> {
        self.entries.iter().rev().find(|entry| &*entry.key == key).map(|entry| self.view(entry))
    }

    /// Returns the entries in source order, duplicate keys included.
    pub fn iter(&self) -> impl Iterator<Item = KeyValEntry<'_, L, B>> {
        self.entries.iter().map(|entry| self.view(entry))
    }

    /// Joins every value recorded under `key` into one new tree.
    ///
    /// The values are copied in source order, with a synthesized chars node holding
    /// `sep` between consecutive ones — the answer for a key meant to accumulate
    /// rather than to override, which is what [`get`](Self::get) assumes. Read the
    /// combined run from the returned tree with `tree.root().children()`.
    ///
    /// Occurrences of `key` that have no value contribute nothing, and an empty `sep`
    /// leaves the separator nodes out. `Ok(None)` means no entry is named `key` at
    /// all; a key whose every occurrence lacks a value gives an empty run rather than
    /// `None`.
    ///
    /// The combined tree's nodes have no annotations (`B` becomes `()`): this is a
    /// reading convenience rather than one of the annotation-supplying helpers.
    ///
    /// # Errors
    ///
    /// [`ExtractError::Build`] if building the combined tree fails.
    pub fn get_combined_with(
        &self,
        key: &str,
        sep: &str,
    ) -> Result<Option<NodeTree<L>>, ExtractError> {
        let values: Vec<NodeSlice<'_, L, B>> = self
            .entries
            .iter()
            .filter(|entry| &*entry.key == key)
            .filter_map(|entry| entry.value)
            .map(|id| self.tree.node(id).children())
            .collect();
        if !self.entries.iter().any(|entry| &*entry.key == key) {
            return Ok(None);
        }

        let root_span = self.tree.root().span().clone();
        let state = self.tree.root().parsing_state().clone();
        let sep_source: Option<Arc<Source<L::SourceOrigin>>> = (!sep.is_empty()).then(|| {
            Arc::new(Source::synthesized(sep, "keyval combined-value separator", root_span.clone()))
        });

        let mut builder = NodeTreeBuilder::new();
        let mut children = Vec::new();
        for (i, value) in values.iter().enumerate() {
            if i > 0 {
                if let Some(sep_source) = &sep_source {
                    let kind = NodeKind::chars(TextContent::Owned(Box::from(sep)));
                    let span = SourceSpan::entire(sep_source);
                    let ext =
                        L::make_node_ext(&kind, &span, &state, builder.staged_children(&[]))?;
                    children.push(builder.add(kind, span, state.clone(), Vec::new(), ext, ())?);
                }
            }
            for node in value.iter() {
                children.push(copy_subtree_into(&mut builder, node, &mut |_| ())?);
            }
        }
        let kind = NodeKind::list();
        let ext =
            L::make_node_ext(&kind, &root_span, &state, builder.staged_children(&children))?;
        let root = builder.add(kind, root_span, state, children, ext, ())?;
        Ok(Some(builder.finish(root)?))
    }

    /// Returns the tree the values are stored in: a root `List` with one `List` child
    /// per entry that has a value.
    ///
    /// It is a derived tree, so its sibling spans do not tile their parent's interior
    /// (module docs).
    pub fn tree(&self) -> &NodeTree<L, B> {
        &self.tree
    }

    /// Consumes this value and returns the tree the values are stored in (see
    /// [`tree`](Self::tree)).
    ///
    /// The keys are dropped along with the entry table, so read whichever ones are
    /// needed first.
    pub fn into_tree(self) -> NodeTree<L, B> {
        self.tree
    }

    fn view<'k>(&'k self, entry: &'k KeyValEntryData) -> KeyValEntry<'k, L, B> {
        KeyValEntry {
            key: &entry.key,
            value: entry.value.map(|id| self.tree.node(id).children()),
        }
    }
}

impl<L: Lang, B> fmt::Debug for KeyVals<L, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut map = f.debug_map();
        for entry in &self.entries {
            map.entry(&&*entry.key, &entry.value);
        }
        map.finish()
    }
}

/// One entry of a [`KeyVals`]: its key and, when it has one, its value nodes.
///
/// Obtained from [`KeyVals::get`], [`KeyVals::keyval`], or [`KeyVals::iter`].
pub struct KeyValEntry<'k, L: Lang, B = ()> {
    key: &'k str,
    value: Option<NodeSlice<'k, L, B>>,
}

impl<'k, L: Lang, B> KeyValEntry<'k, L, B> {
    /// Returns the key.
    ///
    /// [`parse_keyval`] trims whitespace off it; for the run readers it is the
    /// embellishment marker or the field's command name.
    pub fn key(&self) -> &'k str {
        self.key
    }

    /// Returns the value nodes exactly as recorded, or `None` when the entry has no
    /// value at all.
    ///
    /// The two are different: for [`parse_keyval`], `None` is a bare `key` written
    /// without `=`, while an empty slice is `key=` with an explicitly empty value.
    /// [`value_content`](Self::value_content) applies the usual convention of
    /// unwrapping a lone group.
    pub fn value(&self) -> Option<NodeSlice<'k, L, B>> {
        self.value
    }

    /// Returns the value with LaTeX's usual keyval convention applied: when the value
    /// is exactly one group node, as in `legend={a,b}`, its contents; otherwise the
    /// value unchanged.
    ///
    /// This is pylatexenc's `extract_value_group_contents=True`, offered as an
    /// accessor rather than as a setting fixed when the content was read;
    /// [`value`](Self::value) always reports the shape as recorded.
    pub fn value_content(&self) -> Option<NodeSlice<'k, L, B>> {
        let value = self.value?;
        if value.len() == 1 {
            let node = value.first().expect("len 1");
            if node.is_group() {
                return Some(node.children());
            }
        }
        Some(value)
    }
}

impl<L: Lang, B> Clone for KeyValEntry<'_, L, B> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang, B> Copy for KeyValEntry<'_, L, B> {}

impl<L: Lang, B> fmt::Debug for KeyValEntry<'_, L, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyValEntry")
            .field("key", &self.key)
            .field("value", &self.value)
            .finish()
    }
}

#[cfg(test)]
// The owned-content fixture mints node extensions through `Lang::make_node_ext` and
// hands the result to `NodeTreeBuilder::add`, the way the parser does. `Latexlike`'s
// extension type happens to be `()` today, which makes that binding unit-valued;
// keeping it spelled out is what keeps the fixture honest if the type changes.
#[allow(clippy::let_unit_value)]
mod tests {
    use super::*;
    use crate::engine::Language;
    use crate::latexlike::Latexlike;
    use crate::node::NodeTree;

    /// Parse `input` with the latexlike preset plus the minilatex package (strict;
    /// minilatex supplies the typography specials these tests lean on — the seed
    /// itself defines only `\begin`/`\end`).
    fn parse(input: &str) -> NodeTree<Latexlike> {
        use crate::error::Recovery;
        use crate::latexlike::minidefs::minilatex_package;
        use crate::latexlike::LatexlikeDriver;
        use crate::state::ParsingState;
        let language: Language<Latexlike> = Language::new(
            LatexlikeDriver::new(Recovery::Strict),
            ParsingState::lang_initial_with_packages([minilatex_package()]).expect("seed state"),
        );
        let result = language.parse(input).expect("test inputs parse cleanly");
        assert!(result.diagnostics.is_empty(), "unexpected diagnostics: {:?}", result.diagnostics);
        result.tree
    }

    fn texts(slice: NodeSlice<'_, Latexlike>) -> Vec<String> {
        slice.iter().map(|node| node.span_content().into()).collect()
    }

    // --- owned-content input (the transform-created / non-tiled-parse shape) ----------
    //
    // Every helper must answer as documented when chars content is **owned** rather
    // than span-backed: that is what a transform pass stages, and what a parse of a
    // language with `Lang::OBEYS_SPAN_TILING = false` records for multi-token content.

    /// One child of the [`owned_tree`] fixture.
    enum Part<'t> {
        /// A chars node carrying owned text — its span points at the fixture's stub
        /// source, which spells something else.
        Owned(&'t str),
        /// A parsed subtree, copied in with its own spans and payloads.
        Copied(NodeRef<'t, Latexlike>),
    }

    /// A hand-built tree in the shape a transform pass — or a parse of a language
    /// with `Lang::OBEYS_SPAN_TILING = false` — produces: the chars nodes hold owned
    /// text, and their spans are provenance coordinates that do **not** spell that
    /// text out (the stub source reads `<generated>`), so a helper that reads content
    /// through a span cannot pass here by accident.
    fn owned_tree(parts: &[Part<'_>]) -> NodeTree<Latexlike> {
        use crate::state::ParsingState;
        let source: Arc<Source> = Arc::new(Source::new("<generated>"));
        let span = SourceSpan::new(&source, 0..source.content().len());
        let state = Arc::new(ParsingState::<Latexlike>::lang_initial().expect("seed state"));

        let mut builder: NodeTreeBuilder<Latexlike> = NodeTreeBuilder::new();
        let mut children = Vec::new();
        for part in parts {
            children.push(match part {
                Part::Owned(text) => {
                    let kind = NodeKind::chars(TextContent::Owned((*text).into()));
                    let ext = <Latexlike as Lang>::make_node_ext(
                        &kind,
                        &span,
                        &state,
                        builder.staged_children(&[]),
                    )
                    .expect("mint node ext");
                    builder
                        .add(kind, span.clone(), state.clone(), Vec::new(), ext, ())
                        .expect("stage an owned chars node")
                }
                Part::Copied(node) => copy_subtree_into(&mut builder, *node, &mut |_| ())
                    .expect("copy a parsed subtree"),
            });
        }
        let kind = NodeKind::list();
        let ext =
            <Latexlike as Lang>::make_node_ext(&kind, &span, &state, builder.staged_children(&children))
                .expect("mint node ext");
        let root = builder.add(kind, span, state, children, ext, ()).expect("stage the root");
        builder.finish(root).expect("the fixture satisfies the all-trees law")
    }

    // --- content_as_chars -------------------------------------------------------------

    #[test]
    fn content_as_chars_single_chars_node_is_borrowed() {
        let tree = parse("my-label");
        let content = content_as_chars(tree.root().children()).unwrap();
        assert_eq!(content, "my-label");
        assert!(matches!(content, Cow::Borrowed(_)));
    }

    #[test]
    fn content_as_chars_recurses_groups_and_skips_comments() {
        // Group delimiters dropped, nested groups flattened (`\item[{*}]`-style
        // shapes), comments skipped (with their post-space indentation).
        let tree = parse("ab{cd{e}}f");
        assert_eq!(content_as_chars(tree.root().children()).unwrap(), "abcdef");

        let tree = parse("ab%c\ncd");
        let content = content_as_chars(tree.root().children()).unwrap();
        assert_eq!(content, "abcd");
        assert!(matches!(content, Cow::Owned(_)));

        let tree = parse("");
        assert_eq!(content_as_chars(tree.root().children()).unwrap(), "");
    }

    #[test]
    fn content_as_chars_reads_owned_chars_content() {
        // Content comes from the node's data; the span spells something else entirely.
        let tree = owned_tree(&[Part::Owned("my-label")]);
        let content = content_as_chars(tree.root().children()).unwrap();
        assert_eq!(content, "my-label");
        // Owned content is borrowed from the tree just like span-backed content: the
        // documented zero-copy answer for a single contiguous piece holds.
        assert!(matches!(content, Cow::Borrowed(_)));
        assert_eq!(tree.root().child(0).unwrap().span_content(), "<generated>");

        // Mixed input: owned chars around a copied group, whose interior flattens.
        let parsed = parse("{x,y}");
        let tree = owned_tree(&[
            Part::Owned("a"),
            Part::Copied(parsed.root().child(0).unwrap()),
            Part::Owned("b"),
        ]);
        assert_eq!(content_as_chars(tree.root().children()).unwrap(), "ax,yb");
    }

    #[test]
    fn content_as_chars_rejects_callables() {
        // `--` is a base-package typography specials — a Callable node.
        let tree = parse("a--b");
        let error = content_as_chars(tree.root().children()).unwrap_err();
        let ExtractError::NonCharsContent { node } = error else {
            panic!("expected NonCharsContent, got {error:?}");
        };
        assert!(tree.node(node).is_callable());
    }

    // --- split_at_chars ---------------------------------------------------------------

    #[test]
    fn split_at_chars_groups_protect_their_interior() {
        // pylatexenc's own docstring example (`\cite`-style argument content).
        let tree = parse("key1,key2,my{special,key},keyN");
        let split = split_at_chars_drop_annotations(tree.root().children(), ",").unwrap();
        assert_eq!(split.len(), 4);
        assert_eq!(texts(split.segment(0).unwrap()), ["key1"]);
        assert_eq!(texts(split.segment(1).unwrap()), ["key2"]);
        assert_eq!(texts(split.segment(2).unwrap()), ["my", "{special,key}"]);
        assert_eq!(texts(split.segment(3).unwrap()), ["keyN"]);
        // The protected group's interior is intact — one chars child, comma included.
        let group = split.segment(2).unwrap().get(1).unwrap();
        assert!(group.is_group());
        assert_eq!(content_as_chars(group.children()).unwrap(), "special,key");
        // segments() iterates the same views.
        assert_eq!(split.segments().count(), 4);
    }

    #[test]
    fn split_segments_carry_exact_spans() {
        let tree = parse("key1,my{a,b}tail");
        let split = split_at_chars_drop_annotations(tree.root().children(), ",").unwrap();
        assert_eq!(split.len(), 2);
        // Boundary partials are span-backed sub-spans into the *same* source.
        let seg0 = split.segment(0).unwrap();
        assert_eq!(seg0.span().unwrap().range(), 0..4);
        assert_eq!(seg0.source_text(), Some("key1"));
        let seg1 = split.segment(1).unwrap();
        assert_eq!(seg1.span().unwrap().range(), 5..16);
        assert_eq!(seg1.source_text(), Some("my{a,b}tail"));
        assert!(Arc::ptr_eq(
            seg1.span().unwrap().source(),
            tree.root().span().source()
        ));
    }

    #[test]
    fn split_at_chars_drops_empty_segments() {
        let tree = parse("a,,b");
        assert_eq!(split_at_chars_drop_annotations(tree.root().children(), ",").unwrap().len(), 2);
        let tree = parse(",a,");
        let split = split_at_chars_drop_annotations(tree.root().children(), ",").unwrap();
        assert_eq!(split.len(), 1);
        assert_eq!(texts(split.segment(0).unwrap()), ["a"]);
        let tree = parse(",,");
        assert!(split_at_chars_drop_annotations(tree.root().children(), ",").unwrap().is_empty());
    }

    #[test]
    fn split_at_chars_multi_char_separator_and_errors() {
        let tree = parse("a::b::c");
        let split = split_at_chars_drop_annotations(tree.root().children(), "::").unwrap();
        assert_eq!(split.len(), 3);
        assert_eq!(
            split.segments().map(|s| s.source_text().unwrap().to_string()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );

        assert_eq!(
            split_at_chars_drop_annotations(tree.root().children(), "").unwrap_err(),
            ExtractError::EmptySeparator
        );
    }

    #[test]
    fn split_of_empty_or_separator_free_input() {
        let tree = parse("");
        let split = split_at_chars_drop_annotations(tree.root().children(), ",").unwrap();
        assert!(split.is_empty());
        assert!(split.segment(0).is_none());

        // No separator at all: one segment, the whole run.
        let tree = parse("a{b}c");
        let split = split_at_chars_drop_annotations(tree.root().children(), ",").unwrap();
        assert_eq!(split.len(), 1);
        assert_eq!(split.segment(0).unwrap().source_text(), Some("a{b}c"));
    }

    #[test]
    fn split_segments_are_ordinary_node_lists() {
        // The 7.8 uniformity goal: a segment feeds every other helper.
        let tree = parse("a=1,b={x,y}");
        let split = split_at_chars_drop_annotations(tree.root().children(), ",").unwrap();
        let resplit = split_at_chars_drop_annotations(split.segment(0).unwrap(), "=").unwrap();
        assert_eq!(resplit.len(), 2);
        assert_eq!(content_as_chars(resplit.segment(1).unwrap()).unwrap(), "1");
        // Document-order traversal works on the derived tree too.
        assert!(split.tree().descendants().count() >= 4);
        // Copied nodes keep original spans; the derived tree is self-contained.
        let owned: NodeTree<Latexlike> = split.into_tree();
        assert_eq!(owned.root().child(1).unwrap().children().source_text(), Some("b={x,y}"));
    }

    #[test]
    fn split_partials_of_owned_content_keep_whole_node_provenance() {
        // Materialized trees have owned chars content: no byte mapping to subdivide, so
        // a boundary partial keeps the whole original node's span (documented).
        let tree = parse("ab,cd").materialize();
        let split = split_at_chars_drop_annotations(tree.root().children(), ",").unwrap();
        assert_eq!(split.len(), 2);
        let seg0 = split.segment(0).unwrap();
        assert_eq!(seg0.first().unwrap().chars(), Some("ab"));
        assert_eq!(seg0.span().unwrap().range(), 0..5); // whole "ab,cd" chars node span
    }

    #[test]
    fn split_at_chars_cuts_owned_content_and_keeps_node_provenance() {
        // "k1,my" + {x,y} + "tail,k3", with the chars content owned.
        let parsed = parse("{x,y}").materialize();
        let group = parsed.root().child(0).unwrap();
        let tree =
            owned_tree(&[Part::Owned("k1,my"), Part::Copied(group), Part::Owned("tail,k3")]);
        let split = split_at_chars(tree.root().children(), ",", |part| {
            part.partial_text().map(String::from)
        })
        .unwrap();
        assert_eq!(split.len(), 3);

        // The cut text comes from the owned content, and reaches the callback.
        assert_eq!(content_as_chars(split.segment(0).unwrap()).unwrap(), "k1");
        assert_eq!(content_as_chars(split.segment(1).unwrap()).unwrap(), "myx,ytail");
        assert_eq!(content_as_chars(split.segment(2).unwrap()).unwrap(), "k3");
        assert_eq!(
            *split.segment(0).unwrap().first().unwrap().annotation(),
            Some("k1".to_string())
        );

        // Provenance of a partial of owned content: the whole original node's span,
        // as documented — so the segment's coordinates cover the original node, and
        // its `source_text` is what those coordinates spell, not the segment's text.
        let seg0 = split.segment(0).unwrap();
        assert_eq!(seg0.first().unwrap().chars(), Some("k1"));
        assert_eq!(seg0.span().unwrap().range(), 0..11);
        assert_eq!(seg0.source_text(), Some("<generated>"));

        // A run mixing the stub source with the copied group's source has no
        // single-source covering span — the documented `None`.
        assert_eq!(split.segment(1).unwrap().span(), None);
        assert_eq!(split.segment(1).unwrap().source_text(), None);
    }

    // --- parse_keyval -----------------------------------------------------------------

    #[test]
    fn parse_keyval_entries_in_source_order() {
        let tree = parse("width=2cm,legend={a,b},draft,label=");
        let keyvals = parse_keyval_drop_annotations(tree.root().children()).unwrap();
        assert_eq!(keyvals.len(), 4);

        let width = keyvals.keyval(0).unwrap();
        assert_eq!(width.key(), "width");
        assert_eq!(content_as_chars(width.value().unwrap()).unwrap(), "2cm");

        // Grouped values protect their commas; value() is raw, value_content() unwraps
        // the lone group (the documented keyval convention as an accessor, not a knob).
        let legend = keyvals.get("legend").unwrap();
        assert_eq!(texts(legend.value().unwrap()), ["{a,b}"]);
        assert_eq!(content_as_chars(legend.value_content().unwrap()).unwrap(), "a,b");
        assert_eq!(legend.value_content().unwrap().len(), 1); // the "a,b" chars node

        // `draft` (no `=`): no value. `label=` (explicit empty): empty value.
        assert!(keyvals.get("draft").unwrap().value().is_none());
        let label = keyvals.get("label").unwrap();
        assert!(label.value().unwrap().is_empty());
        assert!(keyvals.get("nonsense").is_none());
        assert_eq!(keyvals.keyval(4).map(|_| ()), None);
    }

    #[test]
    fn parse_keyval_trims_keys_and_flattens_grouped_keys() {
        let tree = parse(" spaced key = v ,{grouped}=w");
        let keyvals = parse_keyval_drop_annotations(tree.root().children()).unwrap();
        assert_eq!(keyvals.keyval(0).unwrap().key(), "spaced key");
        // Values stay raw — leading whitespace node included.
        assert_eq!(content_as_chars(keyvals.keyval(0).unwrap().value().unwrap()).unwrap(), " v ");
        assert_eq!(keyvals.keyval(1).unwrap().key(), "grouped");
        // Only the *first* top-level `=` splits: later ones stay in the value.
        let tree = parse("a=b=c");
        let keyvals = parse_keyval_drop_annotations(tree.root().children()).unwrap();
        assert_eq!(content_as_chars(keyvals.get("a").unwrap().value().unwrap()).unwrap(), "b=c");
    }

    #[test]
    fn parse_keyval_duplicates_last_wins_via_get() {
        let tree = parse("a=1,b=9,a=2");
        let keyvals = parse_keyval_drop_annotations(tree.root().children()).unwrap();
        assert_eq!(keyvals.len(), 3); // duplicates preserved
        assert_eq!(content_as_chars(keyvals.get("a").unwrap().value().unwrap()).unwrap(), "2");
        assert_eq!(content_as_chars(keyvals.keyval(0).unwrap().value().unwrap()).unwrap(), "1");
        let keys: Vec<_> = keyvals.iter().map(|entry| entry.key().to_string()).collect();
        assert_eq!(keys, ["a", "b", "a"]);
    }

    #[test]
    fn parse_keyval_rejects_non_chars_keys() {
        let tree = parse("a--b=1");
        assert!(matches!(
            parse_keyval_drop_annotations(tree.root().children()),
            Err(ExtractError::NonCharsContent { .. })
        ));
    }

    #[test]
    fn get_combined_with_joins_all_occurrences() {
        let tree = parse("a=1,b=x,a={2,3}");
        let keyvals = parse_keyval_drop_annotations(tree.root().children()).unwrap();
        let combined = keyvals.get_combined_with("a", ";").unwrap().unwrap();
        let run = combined.root().children();
        assert_eq!(run.len(), 3);
        assert_eq!(run.get(0).unwrap().chars(), Some("1"));
        assert_eq!(run.get(1).unwrap().chars(), Some(";")); // synthesized separator
        assert!(run.get(2).unwrap().is_group());
        assert_eq!(content_as_chars(run.get(2).unwrap().children()).unwrap(), "2,3");
        // The separator node's provenance is a synthesized source carrying `sep`.
        assert_eq!(run.get(1).unwrap().span_content(), ";");

        // Absent key: None. Valueless occurrences contribute nothing. Empty sep: no
        // separator nodes.
        assert!(keyvals.get_combined_with("zzz", ";").unwrap().is_none());
        let tree = parse("a,a=1");
        let keyvals = parse_keyval_drop_annotations(tree.root().children()).unwrap();
        let combined = keyvals.get_combined_with("a", ";").unwrap().unwrap();
        assert_eq!(combined.root().children().len(), 1);
        let tree = parse("a=1,a=2");
        let keyvals = parse_keyval_drop_annotations(tree.root().children()).unwrap();
        let combined = keyvals.get_combined_with("a", "").unwrap().unwrap();
        assert_eq!(texts(combined.root().children()), ["1", "2"]);
    }

    #[test]
    fn keyval_reads_owned_content() {
        // "width=2cm,legend=" + {a,b} + ",draft", all chars content owned.
        let parsed = parse("{a,b}").materialize();
        let group = parsed.root().child(0).unwrap();
        let tree = owned_tree(&[
            Part::Owned("width=2cm, legend ="),
            Part::Copied(group),
            Part::Owned(",draft"),
        ]);
        let keyvals = parse_keyval_drop_annotations(tree.root().children()).unwrap();
        let keys: Vec<_> = keyvals.iter().map(|entry| entry.key().to_string()).collect();
        assert_eq!(keys, ["width", "legend", "draft"]);
        // Keys flatten and trim out of owned text; values keep their raw shape.
        assert_eq!(
            content_as_chars(keyvals.get("width").unwrap().value().unwrap()).unwrap(),
            "2cm"
        );
        // The lone-group value unwraps, and its (owned) interior flattens.
        assert_eq!(
            content_as_chars(keyvals.get("legend").unwrap().value_content().unwrap()).unwrap(),
            "a,b"
        );
        assert!(keyvals.get("draft").unwrap().value().is_none());

        // Combining occurrences copies owned values through unchanged.
        let tree = owned_tree(&[Part::Owned("a=1,a=2")]);
        let keyvals = parse_keyval_drop_annotations(tree.root().children()).unwrap();
        let combined = keyvals.get_combined_with("a", ";").unwrap().unwrap();
        assert_eq!(content_as_chars(combined.root().children()).unwrap(), "1;2");
    }

    // --- split_embellishments / split_tack_on_fields ------------------------------------
    // (The positive paths run end-to-end in the parser test modules,
    // `constructs::embellishments_parser` / `constructs::tack_on_parser`.)

    #[test]
    fn run_readers_skip_noise_and_reject_unexpected_content() {
        // Plain content is neither noise nor an entry of the expected shape.
        let tree = parse("a{b}");
        assert!(matches!(
            split_embellishments_drop_annotations(tree.root().children()),
            Err(ExtractError::UnexpectedContent { .. })
        ));
        assert!(matches!(
            split_tack_on_fields_drop_annotations(tree.root().children()),
            Err(ExtractError::UnexpectedContent { .. })
        ));

        // Empty input and pure noise (whitespace, comments) read as no entries.
        let tree = parse("");
        assert!(split_embellishments_drop_annotations(tree.root().children()).unwrap().is_empty());
        assert!(split_tack_on_fields_drop_annotations(tree.root().children()).unwrap().is_empty());
        let tree = parse(" %c\n ");
        assert!(split_embellishments_drop_annotations(tree.root().children()).unwrap().is_empty());
        assert!(split_tack_on_fields_drop_annotations(tree.root().children()).unwrap().is_empty());
    }

    #[test]
    fn run_readers_read_owned_content() {
        // Embellishments: the owned whitespace node is noise, the copied group is the
        // entry, and its owned content reads back through the value nodes.
        let parsed = parse("{p}").materialize();
        let group = parsed.root().child(0).unwrap();
        let tree = owned_tree(&[Part::Owned("   "), Part::Copied(group)]);
        let fields = split_embellishments_drop_annotations(tree.root().children()).unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields.keyval(0).unwrap().key(), "{");
        assert_eq!(
            content_as_chars(fields.keyval(0).unwrap().value().unwrap()).unwrap(),
            "p"
        );

        // Tack-on fields: the key is the recorded command name, the value the
        // argument's (owned) content.
        let parsed = parse("\\emph{x}").materialize();
        let emph = parsed.root().child(0).unwrap();
        let tree = owned_tree(&[Part::Owned(" "), Part::Copied(emph)]);
        let fields = split_tack_on_fields_drop_annotations(tree.root().children()).unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields.keyval(0).unwrap().key(), "emph");
        assert_eq!(
            content_as_chars(fields.keyval(0).unwrap().value().unwrap()).unwrap(),
            "x"
        );
    }

    // --- annotation minting (the producer triples) --------------------------------------

    #[test]
    fn split_at_chars_general_form_mints_from_part_facts() {
        // "ab,{c}d": segment 0 = partial "ab"; segment 1 = copied group + partial "d".
        let tree = parse("ab,{c}d");
        let chars_id = tree.root().child(0).unwrap().id();
        let group_id = tree.root().child(1).unwrap().id();

        #[derive(Clone, Debug, PartialEq, Eq)]
        struct Mint {
            original: Option<crate::node::NodeId>,
            partial: Option<String>,
            segment: Option<usize>,
        }
        let split = split_at_chars(tree.root().children(), ",", |part| Mint {
            original: part.original().map(|node| node.id()),
            partial: part.partial_text().map(String::from),
            segment: part.segment_index(),
        })
        .unwrap();

        // The root and the segment wrappers are synthesized: no original.
        let root = split.tree().root();
        assert_eq!(
            *root.annotation(),
            Mint { original: None, partial: None, segment: None }
        );
        assert_eq!(
            *root.child(0).unwrap().annotation(),
            Mint { original: None, partial: None, segment: Some(0) }
        );

        // Segment 0: the partial "ab", cut out of the input's first chars node.
        let ab = split.segment(0).unwrap().first().unwrap();
        assert_eq!(
            *ab.annotation(),
            Mint {
                original: Some(chars_id),
                partial: Some("ab".into()),
                segment: Some(0)
            }
        );

        // Segment 1: the copied group derives from the input group (not a
        // partial), its child from the inner chars node; then the partial "d".
        let seg1 = split.segment(1).unwrap();
        let group = seg1.get(0).unwrap();
        assert_eq!(
            *group.annotation(),
            Mint { original: Some(group_id), partial: None, segment: Some(1) }
        );
        assert!(group.annotation().partial.is_none());
        let inner = group.child(0).unwrap();
        assert_eq!(
            inner.annotation().original,
            Some(tree.root().child(1).unwrap().child(0).unwrap().id())
        );
        // "d" is its own chars node (runs break at the group): a whole copy,
        // not a partial.
        let d = seg1.get(1).unwrap();
        assert_eq!(d.annotation().partial, None);
        assert_eq!(d.annotation().original, Some(tree.root().child(2).unwrap().id()));
    }

    #[test]
    fn split_at_chars_keep_annotations_clones_through() {
        // Annotate the input first (A = u32: the node's own index), then split:
        // copies and partials keep their original's annotation, synthesized
        // wrappers get the default.
        let tree = parse("x,{y}").annotate(|node| node.id().index() as u32 + 100);
        let split = split_at_chars_keep_annotations(tree.root().children(), ",").unwrap();

        let x = split.segment(0).unwrap().first().unwrap();
        assert_eq!(*x.annotation(), *tree.root().child(0).unwrap().annotation());
        let group = split.segment(1).unwrap().first().unwrap();
        assert_eq!(*group.annotation(), *tree.root().child(1).unwrap().annotation());
        // Synthesized wrapper/root: A::default().
        assert_eq!(*split.tree().root().annotation(), 0);
        assert_eq!(*split.tree().root().child(0).unwrap().annotation(), 0);
    }

    #[test]
    fn producers_accept_any_input_annotation_type() {
        // Input genericity (T5-A8 rider): an annotated tree splits and re-splits.
        #[derive(Clone, Debug, PartialEq, Eq)]
        struct Stage(&'static str);

        let tree = parse("a=1,b={x,y}").annotate(|_| Stage("analysis"));
        let keyvals = parse_keyval(tree.root().children(), |part| {
            (part.entry_index(), part.original().is_some())
        })
        .unwrap();
        assert_eq!(keyvals.len(), 2);
        // Entry 0's value nodes carry entry index 0; the value-list wrapper is
        // synthesized (original = false).
        let value = keyvals.keyval(0).unwrap().value().unwrap();
        assert_eq!(*value.first().unwrap().annotation(), (Some(0), true));
        let wrapper = value.first().unwrap().parent().unwrap();
        assert_eq!(*wrapper.annotation(), (Some(0), false));
        // Entry 1's grouped value carries entry index 1.
        let value1 = keyvals.get("b").unwrap().value().unwrap();
        assert_eq!(value1.first().unwrap().annotation().0, Some(1));

        // And the output composes into the next producer with its own A.
        let resplit =
            split_at_chars(value1.first().unwrap().children(), ",", |part| {
                part.original().map(|n| *n.annotation())
            })
            .unwrap();
        assert_eq!(resplit.len(), 2);
        assert_eq!(
            *resplit.segment(0).unwrap().first().unwrap().annotation(),
            Some((Some(1), true))
        );
    }

    #[test]
    fn keyval_partials_report_their_cut_text() {
        // parse_keyval cuts at `,` and `=`: the "a" key side is flattened (no
        // node), but the value partials mint with their cut text.
        let tree = parse("k=start,rest");
        let keyvals = parse_keyval(tree.root().children(), |part| {
            (part.is_partial(), part.partial_text().map(String::from))
        })
        .unwrap();
        let value = keyvals.get("k").unwrap().value().unwrap();
        assert_eq!(
            *value.first().unwrap().annotation(),
            (true, Some("start".to_string()))
        );
    }

    #[test]
    fn run_reader_triples_mint_annotations() {
        // split_embellishments: groups read as entries (marker = the opening
        // delimiter); their content copies mint with the entry index.
        let tree = parse("{p}{q}");
        let fields = split_embellishments(tree.root().children(), |part| {
            (part.entry_index(), part.original().is_some())
        })
        .unwrap();
        assert_eq!(fields.len(), 2);
        let q = fields.keyval(1).unwrap().value().unwrap();
        assert_eq!(*q.first().unwrap().annotation(), (Some(1), true));
        assert_eq!(*fields.tree().root().annotation(), (None, false));

        let kept = split_embellishments_keep_annotations(
            parse("{p}").annotate(|_| 7u32).root().children(),
        )
        .unwrap();
        assert_eq!(*kept.keyval(0).unwrap().value().unwrap().first().unwrap().annotation(), 7);

        // split_tack_on_fields: a zero-argument field records an entry with no
        // value; only the root is synthesized.
        let tree = parse(" -- ");
        let mut calls = 0usize;
        let fields = split_tack_on_fields(tree.root().children(), |part| {
            calls += 1;
            part.entry_index()
        })
        .unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields.keyval(0).unwrap().key(), "--");
        assert!(fields.keyval(0).unwrap().value().is_none());
        assert_eq!(calls, 1); // the root only
        assert_eq!(*fields.tree().root().annotation(), None);
        let kept =
            split_tack_on_fields_keep_annotations(tree.annotate(|_| 3u8).root().children())
                .unwrap();
        assert_eq!(*kept.tree().root().annotation(), 0); // synthesized root: default
    }

    #[test]
    fn helpers_compose_over_argument_content() {
        // The end-to-end shape the package exists for: a callable argument's content,
        // split and flattened — here via the environment name-group stand-in `{…}`.
        let tree = parse("{k1,k2{x,y},k3}");
        let group = tree.root().child(0).unwrap();
        let split = split_at_chars_drop_annotations(group.children(), ",").unwrap();
        assert_eq!(split.len(), 3);
        let keys: Vec<_> = split
            .segments()
            .map(|segment| content_as_chars(segment).map(|k| k.into_owned()))
            .collect::<Result<_, _>>()
            .unwrap();
        // The protected group's interior flattens comma included — the split never
        // entered it.
        assert_eq!(keys, ["k1", "k2x,y", "k3"]);
    }
}
