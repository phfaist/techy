//! Subtree copying: re-stage a finished tree's node (with its whole subtree) into a
//! [`NodeTreeBuilder`] — the cross-tree machinery behind the [`extract`](super::extract)
//! helpers.
//!
//! Copying is *by value*: the new tree owns fresh `NodeData` (spans, states, specs, and
//! ext payloads are `Arc`-shared or cloned), and the copies get **new ids** in the new
//! layout. What makes this non-trivial is the two-phase region contract: a finished
//! callable's `ParsedArguments`/`ParsedSlots` regions are *resolved* (global node-index
//! ranges of the source tree), while [`NodeTreeBuilder::add`] requires *staged* records —
//! so each region is translated back to staging coordinates (child offsets +
//! [`ContentNodes`] designations in new-`BuildId` terms), and the builder's `finish()`
//! re-resolves them for the new layout. Crate-internal: public consumers reach this
//! through the extract helpers; a public transform surface is a later phase's design.

use alloc::vec::Vec;

use hashbrown::HashMap;

use crate::state::Lang;

use super::arguments::{ChildRegion, ContentNodes};
use super::builder::{BuildId, NodeTreeBuilder};
use super::kind::NodeKind;
use super::node_ref::NodeRef;
use super::NodeBuildError;

/// Stage a copy of `node` and its whole subtree into `builder`, returning the copy's
/// [`BuildId`]. Children are staged bottom-up in source order; callable region records
/// are translated back to staging coordinates (module docs). Copied nodes carry their
/// exts **verbatim** (frozen parse facts — `make_node_ext` never re-runs on copies);
/// the extract-built trees these helpers feed are `A = ()`, so every copy's
/// annotation is `()`.
pub(crate) fn copy_subtree_into<L: Lang>(
    builder: &mut NodeTreeBuilder<L>,
    node: NodeRef<'_, L>,
) -> Result<BuildId, NodeBuildError> {
    // Old node index → new BuildId, for `InChildrenOf` content parents (descendants of
    // the callable, staged before it by the bottom-up order).
    let mut ids: HashMap<u32, BuildId> = HashMap::new();
    copy_node(builder, node, &mut ids)
}

fn copy_node<L: Lang>(
    builder: &mut NodeTreeBuilder<L>,
    node: NodeRef<'_, L>,
    ids: &mut HashMap<u32, BuildId>,
) -> Result<BuildId, NodeBuildError> {
    let mut children = Vec::with_capacity(node.child_count());
    for child in node.children() {
        children.push(copy_node(builder, child, ids)?);
    }

    let kind = match node.kind() {
        NodeKind::Callable(data) => {
            let mut data = (**data).clone();
            let base = node.children().range().start;
            {
                let regions = data
                    .arguments
                    .arguments
                    .iter_mut()
                    .filter_map(|arg| arg.region.as_mut())
                    .chain(data.slots.slots.iter_mut().map(|slot| &mut slot.region));
                for region in regions {
                    *region = restage_region(region, base, node, ids);
                }
            }
            NodeKind::Callable(alloc::boxed::Box::new(data))
        }
        other => other.clone(),
    };

    let id = builder.add(
        kind,
        node.span().clone(),
        node.parsing_state().clone(),
        children,
        node.ext().clone(),
        (),
    )?;
    ids.insert(index_of(node), id);
    Ok(id)
}

/// Translate one resolved region of `callable` back to staging coordinates. `base` is
/// the callable's children-block start in the source tree's layout.
fn restage_region<L: Lang>(
    region: &ChildRegion,
    base: u32,
    callable: NodeRef<'_, L>,
    ids: &HashMap<u32, BuildId>,
) -> ChildRegion {
    let children = region.children();
    let content = region.content_range();
    let parent = region.content_parent();
    let designation = if parent == callable.id() {
        // Region-level content: offsets relative to the region's own start.
        ContentNodes::InRegion(content.start - children.start..content.end - children.start)
    } else {
        let parent_node = callable.tree().node(parent);
        let parent_base = parent_node.children().range().start;
        let new_parent = *ids
            .get(&index_of(parent_node))
            .expect("content parent is a copied descendant (checked by the source tree's builder)");
        ContentNodes::InChildrenOf(
            new_parent,
            content.start - parent_base..content.end - parent_base,
        )
    };
    ChildRegion::new(children.start - base..children.end - base, designation)
}

fn index_of<L: Lang>(node: NodeRef<'_, L>) -> u32 {
    node.id().index() as u32
}
