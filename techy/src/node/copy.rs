//! The level-0 restage primitive ([`NodeTreeBuilder::restage_node`]) — a single-node
//! copy with a per-child replacement mapping — plus the crate-internal bulk subtree
//! copy built on it (the machinery behind the [`extract`](crate::extract) helpers).
//!
//! Restaging is *by value*: the new tree owns fresh node data (spans, states, specs,
//! and ext payloads are `Arc`-shared or cloned), and the staged copies get **new ids**
//! in the new layout. What makes this non-trivial is the two-phase region contract: a
//! finished callable's `ParsedArguments`/`ParsedSlots` regions are *resolved* (global
//! node-index ranges of the source tree), while [`NodeTreeBuilder::add`] requires
//! *staged* records — so each region is translated back to staging coordinates (child
//! offsets + [`ContentNodes`] designations in new-`BuildId` terms), stretched or
//! shrunk per the replacement mapping, and the builder's `finish()` re-resolves them
//! for the new layout.

use alloc::vec::Vec;

use hashbrown::HashMap;

use crate::state::Lang;

use super::arguments::{ChildRegion, ContentNodes};
use super::builder::{BuildId, NodeTreeBuilder};
use super::kind::NodeKind;
use super::node_ref::NodeRef;
use super::tree::NodeId;
use super::NodeBuildError;

impl<L: Lang, A> NodeTreeBuilder<L, A> {
    /// Stage a copy of one node — the **level-0 restage primitive**: `node`'s kind,
    /// span, parsing state, and ext are cloned, its children are replaced per the
    /// given mapping, and a `Callable`'s argument/slot region records are translated
    /// into the staging coordinates of the replacement layout.
    ///
    /// `replacements` maps the node's children **positionally**: entry `i` holds the
    /// already-staged ids that replace child `i` — empty (the child is dropped), one,
    /// or several. The new node's child list is the concatenation of all entries in
    /// order, and each argument/slot region shrinks or grows with the entries of the
    /// children it spanned. A region all of whose children are dropped restages as
    /// provided-with-an-empty-region — dropping every node of an argument does not
    /// flip the argument to absent.
    ///
    /// `content_parents` translates the content-parent node of every
    /// [`ContentNodes::InChildrenOf`] content designation: it receives the old
    /// tree's [`NodeId`] and answers the staged id replacing that node, or `None`
    /// if it has no staged counterpart (an error, see below). The designation's
    /// child-offset range is carried over verbatim relative to the mapped parent —
    /// [`add`](NodeTreeBuilder::add)/[`finish`](NodeTreeBuilder::finish) re-validate
    /// it like any staged record. Nodes without such designations never consult the
    /// mapping (`|_| None` is fine for them).
    ///
    /// **Cross-tree by contract**: `node` may come from *any* tree — this is the
    /// sanctioned splice door for assembling a new tree out of pieces of several
    /// others, and a same-tree assertion may never be added here. The node's ext is
    /// **cloned verbatim, never re-minted**: restaged copies carry frozen parse
    /// facts ([`Lang::make_node_ext`](crate::state::Lang::make_node_ext) runs at
    /// parse staging, and where a transform author calls it explicitly — nowhere
    /// else). `annotation` becomes the staged node's annotation, supplied by the
    /// caller exactly as in [`add`](NodeTreeBuilder::add).
    ///
    /// # Errors
    ///
    /// [`ReplacementsLengthMismatch`](NodeBuildError::ReplacementsLengthMismatch)
    /// unless `replacements` has exactly one entry per child of `node`;
    /// [`ContentParentUnmapped`](NodeBuildError::ContentParentUnmapped) when
    /// `content_parents` answers `None` for a content parent the records need;
    /// plus every [`add`](NodeTreeBuilder::add) contract error (the replacement ids
    /// must be staged and unclaimed, and so on). As with `add`, an `Err` poisons
    /// the builder.
    pub fn restage_node<AOld>(
        &mut self,
        node: NodeRef<'_, L, AOld>,
        replacements: &[Vec<BuildId>],
        content_parents: impl Fn(NodeId) -> Option<BuildId>,
        annotation: A,
    ) -> Result<BuildId, NodeBuildError> {
        self.restage_node_with_content_mapping(
            node,
            replacements,
            |old| content_parents(old).map(ContentParentMapping::Verbatim),
            annotation,
        )
    }

    /// The crate-internal generalization of
    /// [`restage_node`](NodeTreeBuilder::restage_node): the content-parent
    /// mapping additionally chooses per parent whether the designation's
    /// child-offset range is carried verbatim or translated through the
    /// parent's own replacement layout ([`ContentParentMapping`]) — what the
    /// `techy::transform` driver needs when the parent's children were
    /// themselves restaged (dropped/multiplied) during the same pass. The
    /// public primitive is the all-verbatim specialization.
    pub(crate) fn restage_node_with_content_mapping<'a, AOld>(
        &mut self,
        node: NodeRef<'_, L, AOld>,
        replacements: &[Vec<BuildId>],
        content_parents: impl Fn(NodeId) -> Option<ContentParentMapping<'a>>,
        annotation: A,
    ) -> Result<BuildId, NodeBuildError> {
        let n_children = node.child_count();
        if replacements.len() != n_children {
            return Err(NodeBuildError::ReplacementsLengthMismatch {
                children: n_children,
                replacements: replacements.len(),
            });
        }
        // Prefix sums over the replacement lengths: `prefix[i]` is the new child
        // offset where child `i`'s replacement starts; `prefix[n]` is the new child
        // count. This is the whole coordinate translation — an old child offset `k`
        // becomes `prefix[k]`.
        let mut prefix: Vec<u32> = Vec::with_capacity(n_children + 1);
        let mut total: usize = 0;
        prefix.push(0);
        for entry in replacements {
            total += entry.len();
            let Ok(offset) = u32::try_from(total) else {
                return Err(NodeBuildError::TooManyNodes);
            };
            prefix.push(offset);
        }

        let kind = match node.kind() {
            NodeKind::Callable(data) => {
                let mut data = (**data).clone();
                let base = node.children().range().start;
                let regions = data
                    .arguments
                    .arguments
                    .iter_mut()
                    .filter_map(|arg| arg.region.as_mut())
                    .chain(data.slots.slots.iter_mut().map(|slot| &mut slot.region));
                for region in regions {
                    *region = restage_region(region, base, node, &prefix, &content_parents)?;
                }
                NodeKind::Callable(alloc::boxed::Box::new(data))
            }
            other => other.clone(),
        };

        let children: Vec<BuildId> =
            replacements.iter().flat_map(|entry| entry.iter().copied()).collect();
        self.add(
            kind,
            node.span().clone(),
            node.parsing_state().clone(),
            children,
            node.ext().clone(),
            annotation,
        )
    }
}

/// How a [`ContentNodes::InChildrenOf`] content parent maps into the staged
/// layout — the crate-internal parameter of
/// [`NodeTreeBuilder::restage_node_with_content_mapping`].
#[derive(Clone, Copy, Debug)]
pub(crate) enum ContentParentMapping<'a> {
    /// The designation's child-offset range is carried over verbatim relative
    /// to this staged parent (re-validated at `add()`/`finish()`) — the public
    /// [`restage_node`](NodeTreeBuilder::restage_node) contract.
    Verbatim(BuildId),
    /// The parent's own children were restaged per these replacement-length
    /// prefix sums (index `i` = new child offset where old child `i`'s
    /// replacement starts; one final entry for the new child count): the
    /// designation's range is translated through them, so it keeps designating
    /// the replacements of the originally designated children.
    Translate(BuildId, &'a [u32]),
}

/// Translate one resolved region of `callable` into the staging coordinates of the
/// replacement layout. `base` is the callable's children-block start in its own
/// tree's layout; `prefix` holds the replacement-length prefix sums. The old offsets
/// index `prefix` in bounds by the source tree's record invariants (its builder
/// validated every resolved record at `add()`/`finish()`, and resolved records exist
/// only in finished trees).
fn restage_region<'a, L: Lang, AOld>(
    region: &ChildRegion,
    base: u32,
    callable: NodeRef<'_, L, AOld>,
    prefix: &[u32],
    content_parents: &impl Fn(NodeId) -> Option<ContentParentMapping<'a>>,
) -> Result<ChildRegion, NodeBuildError> {
    let children = region.children();
    let content = region.content_range();
    let parent = region.content_parent();
    let new_children =
        prefix[(children.start - base) as usize]..prefix[(children.end - base) as usize];
    let designation = if parent == callable.id() {
        // Region-level content: translate the content offsets through the same
        // prefix sums, then re-base onto the new region's start.
        let content_start = prefix[(content.start - base) as usize];
        let content_end = prefix[(content.end - base) as usize];
        ContentNodes::InRegion(
            content_start - new_children.start..content_end - new_children.start,
        )
    } else {
        // Content inside a descendant: map the parent; the child-offset range is
        // carried verbatim or translated per the mapping (verbatim ranges are
        // re-validated against the mapped parent at add()/finish()).
        let parent_node = callable.tree().node(parent);
        let parent_base = parent_node.children().range().start;
        let child_range = content.start - parent_base..content.end - parent_base;
        match content_parents(parent)
            .ok_or(NodeBuildError::ContentParentUnmapped { parent })?
        {
            ContentParentMapping::Verbatim(new_parent) => {
                ContentNodes::InChildrenOf(new_parent, child_range)
            }
            ContentParentMapping::Translate(new_parent, parent_prefix) => {
                // The range indexes the parent's prefix in bounds by the same
                // record invariants (validated against the parent's child list
                // by the source tree's builder).
                ContentNodes::InChildrenOf(
                    new_parent,
                    parent_prefix[child_range.start as usize]
                        ..parent_prefix[child_range.end as usize],
                )
            }
        }
    };
    Ok(ChildRegion::new(new_children, designation))
}

/// Stage a copy of `node` and its whole subtree into `builder`, returning the copy's
/// [`BuildId`] — the degenerate recursion over
/// [`restage_node`](NodeTreeBuilder::restage_node): children are staged bottom-up in
/// source order, each as the singleton replacement of itself, with the accumulated
/// old-id map as the content-parent mapping. Copied nodes carry their exts
/// **verbatim** (frozen parse facts — `make_node_ext` never re-runs on copies);
/// `annotate` supplies each copy's annotation from the node it was copied from
/// (the extract producers route their annotation-mint callback through here;
/// `&mut |_| ()` for annotation-free trees).
pub(crate) fn copy_subtree_into<'t, L: Lang, AOld, B>(
    builder: &mut NodeTreeBuilder<L, B>,
    node: NodeRef<'t, L, AOld>,
    annotate: &mut impl FnMut(NodeRef<'t, L, AOld>) -> B,
) -> Result<BuildId, NodeBuildError> {
    // Old NodeId → new BuildId, for `InChildrenOf` content parents (descendants of
    // the callable, staged before it by the bottom-up order).
    let mut ids: HashMap<NodeId, BuildId> = HashMap::new();
    copy_node(builder, node, annotate, &mut ids)
}

fn copy_node<'t, L: Lang, AOld, B>(
    builder: &mut NodeTreeBuilder<L, B>,
    node: NodeRef<'t, L, AOld>,
    annotate: &mut impl FnMut(NodeRef<'t, L, AOld>) -> B,
    ids: &mut HashMap<NodeId, BuildId>,
) -> Result<BuildId, NodeBuildError> {
    let mut replacements = Vec::with_capacity(node.child_count());
    for child in node.children() {
        replacements.push(alloc::vec![copy_node(builder, child, annotate, ids)?]);
    }
    let annotation = annotate(node);
    let id = builder.restage_node(node, &replacements, |old| ids.get(&old).copied(), annotation)?;
    ids.insert(node.id(), id);
    Ok(id)
}
