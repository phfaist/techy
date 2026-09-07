//! Copying nodes into a builder: [`NodeTreeBuilder::restage_node`], the single-node
//! copy with a per-child replacement mapping, plus the crate-internal bulk subtree copy
//! built on it (which the [`extract`](crate::extract) helpers use).
//!
//! Each of the two has a **staged-side twin**, which copies out of a builder's staging
//! arena instead of out of a finished tree:
//! [`NodeTreeBuilder::restage_staged_node`] and
//! [`StagedNodes::copy_subtree_into`]. They are what lets a construct parser read an
//! argument's content while the enclosing parse is still running — the tree it would
//! read does not exist yet — through
//! [`ParseContext::content_as_tree`](crate::core::constructs::ParseContext::content_as_tree).
//!
//! Restaging copies *by value*: the new tree owns fresh node data (spans, states,
//! specs, and ext payloads are `Arc`-shared or cloned), and the copies get **new ids**
//! in the new layout.
//!
//! What makes this non-trivial is the two-phase region contract. A finished callable's
//! `ParsedArguments`/`ParsedSlots` regions are *resolved* — global node-index ranges of
//! the source tree — while [`NodeTreeBuilder::add`] requires *staged* records. Each
//! region is therefore translated back into staging coordinates (child offsets plus
//! [`ContentNodes`] designations in new-`BuildId` terms), stretched or shrunk per the
//! replacement mapping, and re-resolved for the new layout by the builder's
//! `finish()`.

use alloc::vec::Vec;

use hashbrown::HashMap;

use crate::state::Lang;

use super::arguments::{ChildRegion, ContentNodes};
use super::builder::{BuildId, NodeTreeBuilder, StagedNodeView, StagedNodes};
use super::kind::NodeKind;
use super::node_ref::NodeRef;
use super::tree::NodeId;
use super::NodeBuildError;

impl<L: Lang, A> NodeTreeBuilder<L, A> {
    /// Stages a copy of one node, with its children replaced per the given mapping.
    ///
    /// The node's kind, span, parsing state, and ext are cloned, and a `Callable`'s
    /// argument and slot region records are translated into the staging coordinates of
    /// the replacement layout. This is the primitive every larger copy is built from,
    /// including the tree-to-tree driver of [`transform`](crate::transform).
    ///
    /// `replacements` maps the node's children **positionally**: entry `i` holds the
    /// already-staged ids that replace child `i` — none of them (the child is dropped),
    /// one, or several. The new node's child list is the concatenation of all entries
    /// in order, and each argument or slot region shrinks or grows with the entries of
    /// the children it spanned. A region all of whose children are dropped is restaged
    /// as provided with an empty region: dropping every node of an argument does not
    /// turn the argument into an absent one.
    ///
    /// `content_parents` translates the content-parent node of every
    /// [`ContentNodes::InChildrenOf`] designation. It receives the old tree's
    /// [`NodeId`] and answers the staged id that replaces that node, or `None` if it
    /// has no staged counterpart (an error — see below). The designation's child-offset
    /// range is carried over verbatim relative to the mapped parent, and
    /// [`add`](NodeTreeBuilder::add) and [`finish`](NodeTreeBuilder::finish) re-check
    /// it like any staged record. A node without such designations never consults the
    /// mapping, so `|_| None` will do for it.
    ///
    /// `node` may come from **any** tree, deliberately: this method is the supported
    /// route for assembling a new tree out of pieces of several others, and no
    /// same-tree check will ever be added here.
    ///
    /// The node's ext is cloned verbatim and never re-minted, so a restaged copy keeps
    /// the parse facts recorded when it was first staged
    /// ([`Lang::make_node_ext`](crate::core::Lang::make_node_ext) runs at parse
    /// staging, and wherever a transform author calls it explicitly — nowhere else).
    /// `annotation` becomes the staged node's annotation, supplied by the caller
    /// exactly as in [`add`](NodeTreeBuilder::add).
    ///
    /// # Errors
    ///
    /// [`ReplacementsLengthMismatch`](NodeBuildError::ReplacementsLengthMismatch)
    /// unless `replacements` has exactly one entry per child of `node`, and
    /// [`ContentParentUnmapped`](NodeBuildError::ContentParentUnmapped) when
    /// `content_parents` answers `None` for a content parent the records need — plus
    /// every contract error of [`add`](NodeTreeBuilder::add), since the replacement
    /// ids must be staged and unclaimed like any other child. As with `add`, an `Err`
    /// poisons the builder.
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
        let prefix = replacement_prefix_sums(replacements)?;

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

    /// Stages a copy of one **staged** node, with its children replaced per the given
    /// mapping — the staged-side twin of
    /// [`restage_node`](NodeTreeBuilder::restage_node).
    ///
    /// The input node is a [`StagedNodeView`] read out of some builder's staging
    /// arena, rather than a [`NodeRef`] into a finished tree. Everything else matches
    /// `restage_node`: the node's kind, span, parsing state, and ext are cloned, and a
    /// `Callable`'s argument and slot region records are translated into the staging
    /// coordinates of the replacement layout. Use it to copy nodes that are still being
    /// built — nodes a parse has staged but not yet frozen into a tree.
    ///
    /// `replacements` maps the node's children **positionally**: entry `i` holds the
    /// already-staged ids that replace child `i` — none of them (the child is dropped),
    /// one, or several. The new node's child list is the concatenation of all entries
    /// in order, and each argument or slot region shrinks or grows with the entries of
    /// the children it spanned. A region all of whose children are dropped is restaged
    /// as provided with an empty region: dropping every node of an argument does not
    /// turn the argument into an absent one.
    ///
    /// `content_parents` translates the content-parent node of every
    /// [`ContentNodes::InChildrenOf`] designation. It receives the input builder's
    /// [`BuildId`] and answers the id that replaces that node in *this* builder, or
    /// `None` if it has no counterpart (an error — see below). The designation's
    /// child-offset range is carried over verbatim relative to the mapped parent, and
    /// [`add`](NodeTreeBuilder::add) and [`finish`](NodeTreeBuilder::finish) re-check
    /// it like any staged record. A node without such designations never consults the
    /// mapping, so `|_| None` will do for it.
    ///
    /// `node` must come from a **different** builder than this one: the view borrows
    /// its own builder's staging storage for the whole call, so a same-builder call
    /// does not compile. Assembling a tree out of pieces of several builders is the
    /// supported use. To reorganize nodes *within* one builder, finish it and restage
    /// out of the tree with [`restage_node`](NodeTreeBuilder::restage_node).
    ///
    /// The nodes read stay staged and unclaimed in their own builder — reading claims
    /// nothing — and the copy is a new node with a new id here.
    ///
    /// The node's ext is cloned verbatim and never re-minted, so a restaged copy keeps
    /// what its language recorded about it when it was first staged
    /// ([`Lang::make_node_ext`](crate::core::Lang::make_node_ext) runs at parse
    /// staging, and wherever a transform author calls it explicitly — nowhere else).
    /// `annotation` becomes the staged copy's annotation, supplied by the caller
    /// exactly as in [`add`](NodeTreeBuilder::add). The input node's own annotation is
    /// not read: annotations live beside the staging arena, and the read views do not
    /// carry them.
    ///
    /// # Errors
    ///
    /// [`ReplacementsLengthMismatch`](NodeBuildError::ReplacementsLengthMismatch)
    /// unless `replacements` has exactly one entry per child of `node`, and
    /// [`StagedContentParentUnmapped`](NodeBuildError::StagedContentParentUnmapped)
    /// when `content_parents` answers `None` for a content parent the records need —
    /// plus every contract error of [`add`](NodeTreeBuilder::add), since the
    /// replacement ids must be staged and unclaimed like any other child. As with
    /// `add`, an `Err` poisons the builder.
    pub fn restage_staged_node(
        &mut self,
        node: StagedNodeView<'_, L>,
        replacements: &[Vec<BuildId>],
        content_parents: impl Fn(BuildId) -> Option<BuildId>,
        annotation: A,
    ) -> Result<BuildId, NodeBuildError> {
        let n_children = node.children().len();
        if replacements.len() != n_children {
            return Err(NodeBuildError::ReplacementsLengthMismatch {
                children: n_children,
                replacements: replacements.len(),
            });
        }
        let prefix = replacement_prefix_sums(replacements)?;

        let kind = match node.kind() {
            NodeKind::Callable(data) => {
                let mut data = (**data).clone();
                let regions = data
                    .arguments
                    .arguments
                    .iter_mut()
                    .filter_map(|arg| arg.region.as_mut())
                    .chain(data.slots.slots.iter_mut().map(|slot| &mut slot.region));
                for region in regions {
                    *region = restage_staged_region(region, &prefix, &content_parents)?;
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

/// Prefix sums over the replacement lengths: `prefix[i]` is the new child offset where
/// old child `i`'s replacement starts, and `prefix[n]` is the new child count. This is
/// the whole coordinate translation both restaging primitives run on — an old child
/// offset `k` becomes `prefix[k]`.
fn replacement_prefix_sums(replacements: &[Vec<BuildId>]) -> Result<Vec<u32>, NodeBuildError> {
    let mut prefix: Vec<u32> = Vec::with_capacity(replacements.len() + 1);
    let mut total: usize = 0;
    prefix.push(0);
    for entry in replacements {
        total += entry.len();
        let Ok(offset) = u32::try_from(total) else {
            return Err(NodeBuildError::TooManyNodes);
        };
        prefix.push(offset);
    }
    Ok(prefix)
}

/// Translate one **staged** region into the staging coordinates of the replacement
/// layout. A staged region's own offsets already index the callable's child list, so
/// there is no children-block base to subtract; `prefix` holds the replacement-length
/// prefix sums. The old offsets index `prefix` in bounds because `add()` checked them
/// against the same child list the caller counted `prefix` from.
fn restage_staged_region(
    region: &ChildRegion,
    prefix: &[u32],
    content_parents: &impl Fn(BuildId) -> Option<BuildId>,
) -> Result<ChildRegion, NodeBuildError> {
    // Invariant: a node staged in a builder has staged regions — `add()` refuses an
    // already-resolved one (`RegionAlreadyResolved`), so no other kind can be in there.
    let (children, content) = region
        .staged()
        .expect("a staged callable's regions are staged (add() rejects resolved ones)");
    let new_children = prefix[children.start as usize]..prefix[children.end as usize];
    let designation = match content {
        // Region-level content: translate the content offsets (which are relative to
        // the region) through the same prefix sums, then re-base onto the new region.
        ContentNodes::InRegion(r) => ContentNodes::InRegion(
            prefix[(children.start + r.start) as usize] - new_children.start
                ..prefix[(children.start + r.end) as usize] - new_children.start,
        ),
        // Content inside one of the region's nodes: map the parent and carry the
        // child-offset range verbatim (re-validated against the mapped parent by
        // add()/finish()).
        ContentNodes::InChildrenOf(parent, r) => ContentNodes::InChildrenOf(
            content_parents(*parent)
                .ok_or(NodeBuildError::StagedContentParentUnmapped { parent: *parent })?,
            r.clone(),
        ),
    };
    Ok(ChildRegion::new(new_children, designation))
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
/// [`BuildId`].
///
/// This is the plain recursion over [`restage_node`](NodeTreeBuilder::restage_node):
/// children are staged bottom-up in source order, each as the single replacement of
/// itself, with the accumulated old-id map serving as the content-parent mapping.
/// Copied nodes keep their exts unchanged, as everywhere else — `make_node_ext` never
/// re-runs on a copy.
///
/// `annotate` supplies each copy's annotation from the node it was copied from (the
/// extract producers pass their annotation-minting callback through here; `&mut |_| ()`
/// for annotation-free trees).
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

impl<'b, L: Lang> StagedNodes<'b, L> {
    /// Stages a copy of the staged node `id` and its whole subtree into `builder`,
    /// returning the copy's [`BuildId`] in that builder.
    ///
    /// This is the staged-side twin of the finished-tree subtree copy: the plain
    /// recursion over
    /// [`restage_staged_node`](NodeTreeBuilder::restage_staged_node), staging children
    /// bottom-up in source order, each as the single replacement of itself, with the
    /// accumulated old-id map serving as the content-parent mapping. It is what makes
    /// a still-being-parsed subtree readable: copy it into a builder of your own,
    /// finish that builder, and every [`extract`](crate::extract) helper applies to
    /// the result.
    ///
    /// `builder` must be a **different** builder than the one this view reads: the view
    /// borrows its own builder's staging storage for the whole call, so passing that
    /// same builder does not compile.
    ///
    /// The copy is by value throughout — the nodes read here stay staged and unclaimed
    /// in their own builder, unchanged, and the copies get new ids in `builder`. Copied
    /// nodes keep their exts unchanged, as everywhere else:
    /// [`Lang::make_node_ext`](crate::core::Lang::make_node_ext) never re-runs on a
    /// copy.
    ///
    /// `annotate` supplies each copy's annotation from the staged node it was copied
    /// from — `&mut |view| Some(view.id())` to record where each copy came from,
    /// `&mut |_| ()` for an annotation-free tree.
    ///
    /// # Errors
    ///
    /// [`ChildNotStaged`](NodeBuildError::ChildNotStaged) naming `id` itself if `id`
    /// was never staged in this view's builder, or naming a child id that was never
    /// staged, plus every error of
    /// [`restage_staged_node`](NodeTreeBuilder::restage_staged_node). An `Err` poisons
    /// `builder`, which must then be abandoned.
    pub fn copy_subtree_into<B>(
        &self,
        id: BuildId,
        builder: &mut NodeTreeBuilder<L, B>,
        annotate: &mut impl FnMut(StagedNodeView<'b, L>) -> B,
    ) -> Result<BuildId, NodeBuildError> {
        // Old BuildId → new BuildId, for `InChildrenOf` content parents (descendants
        // of the callable, staged before it by the bottom-up order).
        let mut ids: HashMap<BuildId, BuildId> = HashMap::new();
        self.copy_staged_node(id, builder, annotate, &mut ids)
    }

    fn copy_staged_node<B>(
        &self,
        id: BuildId,
        builder: &mut NodeTreeBuilder<L, B>,
        annotate: &mut impl FnMut(StagedNodeView<'b, L>) -> B,
        ids: &mut HashMap<BuildId, BuildId>,
    ) -> Result<BuildId, NodeBuildError> {
        let node = self.get(id).ok_or(NodeBuildError::ChildNotStaged { child: id })?;
        let mut replacements = Vec::with_capacity(node.children().len());
        for child in node.children() {
            replacements
                .push(alloc::vec![self.copy_staged_node(*child, builder, annotate, ids)?]);
        }
        let annotation = annotate(node);
        let new =
            builder.restage_staged_node(node, &replacements, |old| ids.get(&old).copied(), annotation)?;
        ids.insert(id, new);
        Ok(new)
    }
}
