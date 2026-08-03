//! Content-extraction helpers over parsed node lists — the pylatexenc-style read
//! package.
//!
//! Free functions over the node read API, deliberately **not** methods of the core node
//! types (decided at the 7.8 checkpoint): the core stays "storage + access", and helpers
//! are added here without touching what a node list *is*. Two input shapes:
//!
//! - **Readers** take any node sequence (`impl IntoIterator<Item = NodeRef>`):
//!   [`content_as_chars`].
//! - **Builders** take a [`NodeSlice`] and return an owned result holding a new
//!   [`NodeTree`]: [`split_at_chars`] (→ [`Split`]), [`parse_keyval`], and the
//!   argument-run readers [`split_embellishments`] / [`split_tack_on_fields`]
//!   (→ [`KeyVals`]). They need the slice's tree anchor (parsing state, source) even
//!   when the slice is empty, which a bare iterator cannot provide.
//!
//! # Builders mint real trees (the 7.8 "builder route")
//!
//! Splitting can cut *through* a chars node (`\cite{key1,my{x,y}z}`), and the source
//! tree is frozen — so, exactly like pylatexenc (whose `split_at_chars` mints new
//! `LatexCharsNode`s), the builder helpers assemble a **new tree**: unaffected nodes are
//! copied wholesale (spans, states, and specs `Arc`-shared; new ids), and boundary
//! partials become fresh `Chars` nodes whose content is span-backed into the *same*
//! source (exact sub-spans, zero-copy text). Segments are then ordinary [`NodeSlice`]
//! views into the result — one node-list currency everywhere, and every helper composes
//! with every other. Two documented approximations at the edges: a partial of an
//! **owned-content** chars node (materialized trees) keeps the whole original node's
//! span as its provenance (there is no byte mapping to subdivide), and partial nodes
//! carry default (not copied) ext data. Result trees are derived views: their sibling
//! spans do *not* tile their parents' interiors (separators are omitted), so they are
//! not subject to `check_tree_invariants`' byte-accounting.
//!
//! ```
//! use techy::core::Language;
//! use techy::extract;
//! use techy::latexlike::Latexlike;
//!
//! let language: Language<Latexlike> = Language::default();
//! let result = language.parse("alpha,beta{x,y},gamma").unwrap();
//!
//! let split = extract::split_at_chars(result.tree.root().children(), ",").unwrap();
//! assert_eq!(split.len(), 3);
//! assert_eq!(split.segment(0).unwrap().source_text(), Some("alpha"));
//! // Grouped content protects its interior — and segments are ordinary node lists,
//! // so every helper composes:
//! assert_eq!(split.segment(1).unwrap().source_text(), Some("beta{x,y}"));
//! assert_eq!(extract::content_as_chars(split.segment(1).unwrap()).unwrap(), "betax,y");
//! ```

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

/// Error of the extraction helpers. These are read-time operations outside any parse
/// run, so there is no tolerant mode: unsuitable content is always a plain `Err`
/// (decided at the 7.8 checkpoint).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtractError {
    /// [`content_as_chars`] (directly, or on a keyval key) met a node that cannot
    /// flatten to characters — anything but chars, comments (skipped), and
    /// group/list containers (recursed).
    NonCharsContent {
        /// The offending node, in the tree the input nodes came from.
        node: NodeId,
    },
    /// The separator passed to [`split_at_chars`] was empty.
    EmptySeparator,
    /// A run-shaped helper ([`split_embellishments`], [`split_tack_on_fields`]) met a
    /// node that is neither skippable noise (whitespace-only chars, comments) nor an
    /// entry of the shape it reads — the input was not the argument-content run the
    /// helper is documented for.
    UnexpectedContent {
        /// The offending node, in the tree the input nodes came from.
        node: NodeId,
    },
    /// Building the result tree failed — the builder surfaced an implementation bug
    /// (e.g. a misbehaving [`Lang::finalize_node`] hook mutating copies into invalid
    /// shapes), never a source-input condition.
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
/// chars node — a byte sub-range of a chars node's logical content. Internal: the
/// public currency is trees and [`NodeSlice`]s (7.8 decision — no second public
/// node-list type).
struct Piece<'t, L: Lang> {
    node: NodeRef<'t, L>,
    /// `Some(sub)` = the sub-range `sub` of the node's chars content; `None` = whole
    /// node. Only ever `Some` for `Chars` nodes.
    part: Option<Range<usize>>,
}

impl<L: Lang> Clone for Piece<'_, L> {
    fn clone(&self) -> Self {
        Piece { node: self.node, part: self.part.clone() }
    }
}

fn whole<L: Lang>(node: NodeRef<'_, L>) -> Piece<'_, L> {
    Piece { node, part: None }
}

/// The piece's chars text, if it is a chars piece (whole chars node or partial).
fn piece_text<'t, L: Lang>(piece: &Piece<'t, L>) -> Option<&'t str> {
    let text = piece.node.chars()?;
    Some(match &piece.part {
        None => text,
        Some(sub) => text
            .get(sub.clone())
            .expect("piece sub-ranges lie on char boundaries of their node's content by construction"),
    })
}

/// The piece's provenance span: the node's span, narrowed to the sub-range for a
/// partial of span-backed content (exact); the whole node's span for a partial of
/// owned content (best available provenance — module docs).
fn piece_span<L: Lang>(piece: &Piece<'_, L>) -> SourceSpan<L::SourceOrigin> {
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

/// The character content of a node sequence, flattened: chars nodes contribute their
/// text, comments are skipped, groups and lists contribute their children recursively
/// (group delimiters dropped), and anything else — a callable — is an error.
///
/// pylatexenc's `get_content_as_chars()`: extracts string arguments from calls like
/// `\label{my-label}` or `\href{https://…}{…}`, including nested-group cases like
/// `\item[{*}]`. Zero-copy (`Cow::Borrowed`) when the flattened text is a single
/// contiguous piece — the common single-chars-node argument.
pub fn content_as_chars<'t, L: Lang>(
    nodes: impl IntoIterator<Item = NodeRef<'t, L>>,
) -> Result<Cow<'t, str>, ExtractError> {
    let mut acc = Acc::Empty;
    for node in nodes {
        collect_chars(node, None, &mut acc)?;
    }
    Ok(acc.finish())
}

/// `content_as_chars` over pieces (keyval keys).
fn pieces_as_chars<'t, L: Lang>(pieces: &[Piece<'t, L>]) -> Result<Cow<'t, str>, ExtractError> {
    let mut acc = Acc::Empty;
    for piece in pieces {
        collect_chars(piece.node, piece.part.clone(), &mut acc)?;
    }
    Ok(acc.finish())
}

fn collect_chars<'t, L: Lang>(
    node: NodeRef<'t, L>,
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
        NodeKind::Group(_) | NodeKind::List { .. } => {
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
fn split_pieces<'t, L: Lang>(
    input: impl IntoIterator<Item = Piece<'t, L>>,
    sep: &str,
    max_split: Option<usize>,
    keep_empty: bool,
) -> Vec<Vec<Piece<'t, L>>> {
    let mut segments: Vec<Vec<Piece<'t, L>>> = Vec::new();
    let mut current: Vec<Piece<'t, L>> = Vec::new();
    let mut splits = 0usize;

    fn flush<'t, L: Lang>(
        current: &mut Vec<Piece<'t, L>>,
        segments: &mut Vec<Vec<Piece<'t, L>>>,
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
fn anchor<L: Lang>(
    slice: &NodeSlice<'_, L>,
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

fn stage_piece<L: Lang>(
    builder: &mut NodeTreeBuilder<L>,
    piece: &Piece<'_, L>,
) -> Result<BuildId, NodeBuildError> {
    let Some(sub) = &piece.part else {
        return copy_subtree_into(builder, piece.node);
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
    builder.add(NodeKind::chars(content), span, piece.node.parsing_state().clone(), Vec::new())
}

/// Stage one segment as a `List` node over its pieces. Empty segments (keyval's
/// explicitly-empty values) anchor at the fallback span/state.
fn stage_segment_list<L: Lang>(
    builder: &mut NodeTreeBuilder<L>,
    pieces: &[Piece<'_, L>],
    fallback_span: &SourceSpan<L::SourceOrigin>,
    fallback_state: &Arc<ParsingState<L>>,
) -> Result<BuildId, NodeBuildError> {
    let mut children = Vec::with_capacity(pieces.len());
    for piece in pieces {
        children.push(stage_piece(builder, piece)?);
    }
    let (span, state) = match (pieces.first(), pieces.last()) {
        (Some(first), Some(last)) => (
            covering(&piece_span(first), &piece_span(last)),
            first.node.parsing_state().clone(),
        ),
        _ => (fallback_span.clone(), fallback_state.clone()),
    };
    builder.add(NodeKind::list(), span, state, children)
}

// --- split_at_chars ---------------------------------------------------------------------

/// Split a run of sibling nodes into segments delimited by `sep` occurrences in
/// **top-level chars nodes** — pylatexenc's `LatexNodeList.split_at_chars`. Grouped
/// content protects its interior: splitting the argument of
/// `\cite{key1,key2,my{special,key},keyN}` at `","` yields four segments, the third
/// being the chars piece `my` followed by the whole `{special,key}` group. Empty
/// segments (adjacent, leading, or trailing separators) are dropped, matching
/// pylatexenc's default.
///
/// The segments live in one new tree owned by the returned [`Split`] (module docs:
/// copies + fresh boundary partials with exact sub-spans); access them as
/// [`NodeSlice`]s via [`Split::segment`]/[`Split::segments`].
///
/// Errors: [`ExtractError::EmptySeparator`] for an empty `sep`;
/// [`ExtractError::Build`] if result-tree construction fails.
pub fn split_at_chars<L: Lang>(
    nodes: NodeSlice<'_, L>,
    sep: &str,
) -> Result<Split<L>, ExtractError> {
    if sep.is_empty() {
        return Err(ExtractError::EmptySeparator);
    }
    let segments = split_pieces(nodes.iter().map(whole), sep, None, false);
    let (anchor_span, anchor_state) = anchor(&nodes);

    let mut builder = NodeTreeBuilder::new();
    let mut lists = Vec::with_capacity(segments.len());
    for segment in &segments {
        lists.push(stage_segment_list(&mut builder, segment, &anchor_span, &anchor_state)?);
    }
    let span = match (segments.first().and_then(|s| s.first()), segments.last().and_then(|s| s.last()))
    {
        (Some(first), Some(last)) => covering(&piece_span(first), &piece_span(last)),
        _ => anchor_span,
    };
    let root = builder.add(NodeKind::list(), span, anchor_state, lists)?;
    Ok(Split { tree: builder.finish(root)? })
}

/// The result of [`split_at_chars`]: the split segments, backed by one owned
/// [`NodeTree`] (root `List`, one `List` child per segment).
pub struct Split<L: Lang> {
    tree: NodeTree<L>,
}

impl<L: Lang> Split<L> {
    /// The number of segments.
    pub fn len(&self) -> usize {
        self.tree.root().child_count()
    }

    /// Whether there are no segments.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Segment `i`'s nodes.
    pub fn segment(&self, i: usize) -> Option<NodeSlice<'_, L>> {
        Some(self.tree.root().child(i)?.children())
    }

    /// The segments, in source order.
    pub fn segments(&self) -> impl Iterator<Item = NodeSlice<'_, L>> {
        self.tree.root().children().iter().map(|list| list.children())
    }

    /// The backing tree: root `List` with one `List` child per segment. A derived
    /// view-tree — sibling spans do not tile (separators are omitted; module docs).
    pub fn tree(&self) -> &NodeTree<L> {
        &self.tree
    }

    /// Consume into the backing tree.
    pub fn into_tree(self) -> NodeTree<L> {
        self.tree
    }
}

impl<L: Lang> fmt::Debug for Split<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Split").field("segments", &self.len()).finish()
    }
}

// --- parse_keyval -----------------------------------------------------------------------

/// Parse a run of sibling nodes as `key1=<value1>,key2=<value2>,…` content —
/// pylatexenc's `parse_keyval_content`. Pairs split at top-level `,`; within a pair the
/// first top-level `=` separates the key from the value; grouped content protects both
/// (`legend={a,b}`). Keys flatten via [`content_as_chars`] and are
/// **whitespace-trimmed** (a deliberate deviation from pylatexenc, which keeps `" a "`
/// verbatim: LaTeX's keyval packages trim, and untrimmed keys are a recurring
/// pylatexenc footgun). Values are recorded **raw and in source order, duplicates
/// preserved** (the 7.8 no-knobs decision): pylatexenc's `repeated_key_aggregate_action`
/// policies are one-line derivations over [`iter`](KeyVals::iter), and
/// [`get`](KeyVals::get) answers the common "effective value" question (last wins —
/// LaTeX override semantics). A key with `=` and nothing after it gets an empty value;
/// a key with no `=` gets none ([`KeyValEntry::value`] is `None` — sharper than
/// pylatexenc, which conflates the two).
///
/// Errors: [`ExtractError::NonCharsContent`] when a key does not flatten to characters;
/// [`ExtractError::Build`] if result-tree construction fails.
pub fn parse_keyval<L: Lang>(nodes: NodeSlice<'_, L>) -> Result<KeyVals<L>, ExtractError> {
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
                )?);
                Some(value_lists.len() - 1)
            }
            None => None,
        };
        entries.push((key.trim().into(), value));
    }

    finish_keyvals(builder, value_lists, entries, anchor_span, anchor_state)
}

/// Assemble a [`KeyVals`] from staged value lists and a `(key, value-list index)`
/// table — the shared tail of [`parse_keyval`], [`split_embellishments`], and
/// [`split_tack_on_fields`].
fn finish_keyvals<L: Lang>(
    mut builder: NodeTreeBuilder<L>,
    value_lists: Vec<BuildId>,
    entries: Vec<(Box<str>, Option<usize>)>,
    anchor_span: SourceSpan<L::SourceOrigin>,
    anchor_state: Arc<ParsingState<L>>,
) -> Result<KeyVals<L>, ExtractError> {
    let root = builder.add(NodeKind::list(), anchor_span, anchor_state, value_lists)?;
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

/// Whether `node` is skippable run noise — a comment, or a whitespace-only chars node
/// (the staged form of pre-entry whitespace in argument regions).
fn is_run_noise<L: Lang>(node: &NodeRef<'_, L>) -> bool {
    match node.kind() {
        NodeKind::Comment { .. } => true,
        NodeKind::Chars { .. } => node
            .chars()
            .is_some_and(|text| text.chars().all(char::is_whitespace)),
        _ => false,
    }
}

/// Read an embellishments argument's content run
/// ([`EmbellishmentsArgumentParser`](crate::constructs::EmbellishmentsArgumentParser))
/// as [`KeyVals`]: one entry per matched embellishment in source order, the **marker**
/// as the key (the wrapper group's opening delimiter — `"^"`, `"_"`, …) and the
/// embellishment's argument nodes as the value, noise-free (the whitespace node a
/// `^ {a}`-style pair stages inside its wrapper is filtered out). Noise between
/// embellishments (whitespace, comments) is skipped; any node that is neither noise
/// nor a group is [`ExtractError::UnexpectedContent`] — feed this helper the
/// argument's *content* nodes ([`NodeRef::argument_content_nodes`]).
///
/// The [`KeyVals`] grammar fits embellishments exactly: duplicate keys preserved in
/// order, [`get`](KeyVals::get) = last occurrence, and
/// [`value_content`](KeyValEntry::value_content) unwraps the usual lone `{…}` value
/// group (`^{ab}` → the `ab` content).
pub fn split_embellishments<L: Lang>(
    nodes: NodeSlice<'_, L>,
) -> Result<KeyVals<L>, ExtractError> {
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
        let pieces: Vec<Piece<'_, L>> = node
            .children()
            .iter()
            .filter(|child| !is_run_noise(child))
            .map(whole)
            .collect();
        value_lists.push(stage_segment_list(&mut builder, &pieces, &anchor_span, &anchor_state)?);
        entries.push((Box::from(marker), Some(value_lists.len() - 1)));
    }
    finish_keyvals(builder, value_lists, entries, anchor_span, anchor_state)
}

/// Read a tack-on fields argument's content run
/// ([`TackOnFieldsArgumentParser`](crate::constructs::TackOnFieldsArgumentParser)) as
/// [`KeyVals`]: one entry per absorbed field invocation in source order, the field's
/// **command name** as the key (`"label"`) and the invocation's argument content as
/// the value, noise-free — the content nodes of every *provided* argument,
/// concatenated in invocation order (for the dominant one-argument field shape:
/// exactly that argument's content). A field invocation providing no argument at all
/// (a zero-argument flag field, or every argument absent) records no value
/// ([`KeyValEntry::value`] is `None`) — sharper than an explicitly empty `\label{}`,
/// whose value is an empty slice, mirroring the keyval `draft` vs. `label=`
/// distinction.
///
/// Noise between fields is skipped; any node that is neither noise nor a callable is
/// [`ExtractError::UnexpectedContent`] — feed this helper the argument's *content*
/// nodes ([`NodeRef::argument_content_nodes`]). Duplicate fields (repeatable `\label`s)
/// are preserved in order; [`get`](KeyVals::get) answers with the last,
/// [`get_combined_with`](KeyVals::get_combined_with) collects them all.
pub fn split_tack_on_fields<L: Lang>(
    nodes: NodeSlice<'_, L>,
) -> Result<KeyVals<L>, ExtractError> {
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
        let mut pieces: Option<Vec<Piece<'_, L>>> = None;
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
                )?);
                Some(value_lists.len() - 1)
            }
            None => None,
        };
        entries.push((Box::from(name), value));
    }
    finish_keyvals(builder, value_lists, entries, anchor_span, anchor_state)
}

struct KeyValEntryData {
    key: Box<str>,
    /// The value `List` node in the backing tree; `None` = the key came without `=`.
    value: Option<NodeId>,
}

/// The result of [`parse_keyval`]: the entries **in source order, duplicates
/// preserved**, with dict-style by-name access ([`get`](KeyVals::get), last occurrence
/// wins) — values are backed by one owned [`NodeTree`]. Lookup scans the entries
/// (keyval lists are small; the `ParsedArguments` no-name-map precedent).
pub struct KeyVals<L: Lang> {
    tree: NodeTree<L>,
    entries: Vec<KeyValEntryData>,
}

impl<L: Lang> KeyVals<L> {
    /// The number of entries (duplicates included).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entry `i`, in source order.
    pub fn keyval(&self, i: usize) -> Option<KeyValEntry<'_, L>> {
        self.entries.get(i).map(|entry| self.view(entry))
    }

    /// The **last** entry named `key` — the effective value under LaTeX keyval override
    /// semantics. Earlier occurrences remain reachable through [`iter`](KeyVals::iter).
    pub fn get(&self, key: &str) -> Option<KeyValEntry<'_, L>> {
        self.entries.iter().rev().find(|entry| &*entry.key == key).map(|entry| self.view(entry))
    }

    /// The entries, in source order (duplicates included).
    pub fn iter(&self) -> impl Iterator<Item = KeyValEntry<'_, L>> {
        self.entries.iter().map(|entry| self.view(entry))
    }

    /// All values recorded under `key` (every occurrence, source order), concatenated
    /// into one new tree with a synthesized separator chars node carrying `sep` between
    /// consecutive values — "collect the values" for cumulative keys. Occurrences
    /// without a value contribute nothing; an empty `sep` omits the separator nodes.
    /// `Ok(None)` when no entry at all is named `key`. The returned tree's root `List`
    /// holds the combined run — read it with `tree.root().children()`.
    pub fn get_combined_with(
        &self,
        key: &str,
        sep: &str,
    ) -> Result<Option<NodeTree<L>>, ExtractError> {
        let values: Vec<NodeSlice<'_, L>> = self
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
                    children.push(builder.add(
                        NodeKind::chars(TextContent::Owned(Box::from(sep))),
                        SourceSpan::entire(sep_source),
                        state.clone(),
                        Vec::new(),
                    )?);
                }
            }
            for node in value.iter() {
                children.push(copy_subtree_into(&mut builder, node)?);
            }
        }
        let root = builder.add(NodeKind::list(), root_span, state, children)?;
        Ok(Some(builder.finish(root)?))
    }

    /// The backing tree: root `List` with one `List` child per value-bearing entry. A
    /// derived view-tree (module docs).
    pub fn tree(&self) -> &NodeTree<L> {
        &self.tree
    }

    /// Consume into the backing tree. The entry table is dropped — extract keys first.
    pub fn into_tree(self) -> NodeTree<L> {
        self.tree
    }

    fn view<'k>(&'k self, entry: &'k KeyValEntryData) -> KeyValEntry<'k, L> {
        KeyValEntry {
            key: &entry.key,
            value: entry.value.map(|id| self.tree.node(id).children()),
        }
    }
}

impl<L: Lang> fmt::Debug for KeyVals<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut map = f.debug_map();
        for entry in &self.entries {
            map.entry(&&*entry.key, &entry.value);
        }
        map.finish()
    }
}

/// One [`KeyVals`] entry, viewed: the trimmed key and the raw value nodes.
pub struct KeyValEntry<'k, L: Lang> {
    key: &'k str,
    value: Option<NodeSlice<'k, L>>,
}

impl<'k, L: Lang> KeyValEntry<'k, L> {
    /// The key (whitespace-trimmed; see [`parse_keyval`]).
    pub fn key(&self) -> &'k str {
        self.key
    }

    /// The raw value nodes: `None` = the key came without `=`; an empty slice = `key=`
    /// with an explicitly empty value.
    pub fn value(&self) -> Option<NodeSlice<'k, L>> {
        self.value
    }

    /// The value with the documented LaTeX keyval convention applied: when the value is
    /// exactly one group node (`legend={a,b}`), its contents; otherwise the raw value.
    /// This is pylatexenc's `extract_value_group_contents=True` as an accessor instead
    /// of a parse-time knob — [`value`](KeyValEntry::value) always keeps the raw shape.
    pub fn value_content(&self) -> Option<NodeSlice<'k, L>> {
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

impl<L: Lang> Clone for KeyValEntry<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang> Copy for KeyValEntry<'_, L> {}

impl<L: Lang> fmt::Debug for KeyValEntry<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyValEntry")
            .field("key", &self.key)
            .field("value", &self.value)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Language;
    use crate::latexlike::Latexlike;
    use crate::node::NodeTree;

    /// Parse `input` with the out-of-the-box latexlike preset (strict; the base package
    /// defines `\begin`/`\end` and the typography specials — no other callables).
    fn parse(input: &str) -> NodeTree<Latexlike> {
        let language: Language<Latexlike> = Language::default();
        let result = language.parse(input).expect("test inputs parse cleanly");
        assert!(result.diagnostics.is_empty(), "unexpected diagnostics: {:?}", result.diagnostics);
        result.tree
    }

    fn texts(slice: NodeSlice<'_, Latexlike>) -> Vec<String> {
        slice.iter().map(|node| node.span_content().into()).collect()
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
        let split = split_at_chars(tree.root().children(), ",").unwrap();
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
        let split = split_at_chars(tree.root().children(), ",").unwrap();
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
        assert_eq!(split_at_chars(tree.root().children(), ",").unwrap().len(), 2);
        let tree = parse(",a,");
        let split = split_at_chars(tree.root().children(), ",").unwrap();
        assert_eq!(split.len(), 1);
        assert_eq!(texts(split.segment(0).unwrap()), ["a"]);
        let tree = parse(",,");
        assert!(split_at_chars(tree.root().children(), ",").unwrap().is_empty());
    }

    #[test]
    fn split_at_chars_multi_char_separator_and_errors() {
        let tree = parse("a::b::c");
        let split = split_at_chars(tree.root().children(), "::").unwrap();
        assert_eq!(split.len(), 3);
        assert_eq!(
            split.segments().map(|s| s.source_text().unwrap().to_string()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );

        assert_eq!(
            split_at_chars(tree.root().children(), "").unwrap_err(),
            ExtractError::EmptySeparator
        );
    }

    #[test]
    fn split_of_empty_or_separator_free_input() {
        let tree = parse("");
        let split = split_at_chars(tree.root().children(), ",").unwrap();
        assert!(split.is_empty());
        assert!(split.segment(0).is_none());

        // No separator at all: one segment, the whole run.
        let tree = parse("a{b}c");
        let split = split_at_chars(tree.root().children(), ",").unwrap();
        assert_eq!(split.len(), 1);
        assert_eq!(split.segment(0).unwrap().source_text(), Some("a{b}c"));
    }

    #[test]
    fn split_segments_are_ordinary_node_lists() {
        // The 7.8 uniformity goal: a segment feeds every other helper.
        let tree = parse("a=1,b={x,y}");
        let split = split_at_chars(tree.root().children(), ",").unwrap();
        let resplit = split_at_chars(split.segment(0).unwrap(), "=").unwrap();
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
        let split = split_at_chars(tree.root().children(), ",").unwrap();
        assert_eq!(split.len(), 2);
        let seg0 = split.segment(0).unwrap();
        assert_eq!(seg0.first().unwrap().chars(), Some("ab"));
        assert_eq!(seg0.span().unwrap().range(), 0..5); // whole "ab,cd" chars node span
    }

    // --- parse_keyval -----------------------------------------------------------------

    #[test]
    fn parse_keyval_entries_in_source_order() {
        let tree = parse("width=2cm,legend={a,b},draft,label=");
        let keyvals = parse_keyval(tree.root().children()).unwrap();
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
        let keyvals = parse_keyval(tree.root().children()).unwrap();
        assert_eq!(keyvals.keyval(0).unwrap().key(), "spaced key");
        // Values stay raw — leading whitespace node included.
        assert_eq!(content_as_chars(keyvals.keyval(0).unwrap().value().unwrap()).unwrap(), " v ");
        assert_eq!(keyvals.keyval(1).unwrap().key(), "grouped");
        // Only the *first* top-level `=` splits: later ones stay in the value.
        let tree = parse("a=b=c");
        let keyvals = parse_keyval(tree.root().children()).unwrap();
        assert_eq!(content_as_chars(keyvals.get("a").unwrap().value().unwrap()).unwrap(), "b=c");
    }

    #[test]
    fn parse_keyval_duplicates_last_wins_via_get() {
        let tree = parse("a=1,b=9,a=2");
        let keyvals = parse_keyval(tree.root().children()).unwrap();
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
            parse_keyval(tree.root().children()),
            Err(ExtractError::NonCharsContent { .. })
        ));
    }

    #[test]
    fn get_combined_with_joins_all_occurrences() {
        let tree = parse("a=1,b=x,a={2,3}");
        let keyvals = parse_keyval(tree.root().children()).unwrap();
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
        let keyvals = parse_keyval(tree.root().children()).unwrap();
        let combined = keyvals.get_combined_with("a", ";").unwrap().unwrap();
        assert_eq!(combined.root().children().len(), 1);
        let tree = parse("a=1,a=2");
        let keyvals = parse_keyval(tree.root().children()).unwrap();
        let combined = keyvals.get_combined_with("a", "").unwrap().unwrap();
        assert_eq!(texts(combined.root().children()), ["1", "2"]);
    }

    // --- split_embellishments / split_tack_on_fields ------------------------------------
    // (The positive paths run end-to-end in the parser test modules,
    // `constructs::embellishments_parser` / `constructs::tack_on_parser`.)

    #[test]
    fn run_readers_skip_noise_and_reject_unexpected_content() {
        // Plain content is neither noise nor an entry of the expected shape.
        let tree = parse("a{b}");
        assert!(matches!(
            split_embellishments(tree.root().children()),
            Err(ExtractError::UnexpectedContent { .. })
        ));
        assert!(matches!(
            split_tack_on_fields(tree.root().children()),
            Err(ExtractError::UnexpectedContent { .. })
        ));

        // Empty input and pure noise (whitespace, comments) read as no entries.
        let tree = parse("");
        assert!(split_embellishments(tree.root().children()).unwrap().is_empty());
        assert!(split_tack_on_fields(tree.root().children()).unwrap().is_empty());
        let tree = parse(" %c\n ");
        assert!(split_embellishments(tree.root().children()).unwrap().is_empty());
        assert!(split_tack_on_fields(tree.root().children()).unwrap().is_empty());
    }

    #[test]
    fn helpers_compose_over_argument_content() {
        // The end-to-end shape the package exists for: a callable argument's content,
        // split and flattened — here via the environment name-group stand-in `{…}`.
        let tree = parse("{k1,k2{x,y},k3}");
        let group = tree.root().child(0).unwrap();
        let split = split_at_chars(group.children(), ",").unwrap();
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
