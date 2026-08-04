//! The restage driver: the [`restage`] entry point and the staging context
//! ([`RestageContext`]) handed to visitors.

use core::marker::PhantomData;

use alloc::vec::Vec;

use hashbrown::HashMap;

use crate::node::{
    BuildId, ContentParentMapping, NodeBuildError, NodeId, NodeRef, NodeTree,
    NodeTreeBuilder,
};
use crate::state::Lang;

use super::{Restage, RestageError, RestageVisitor};

/// Transform a tree by streaming restage: the visitor is invoked **top-down**
/// over the frozen `tree` (root included), staging the output **bottom-up**;
/// the finished tree is returned. See the [module docs](super) for the callback
/// contract, the annotation pathway, and the edit policy.
///
/// The root must restage to exactly one node —
/// [`RootNotSingular`](RestageError::RootNotSingular) otherwise.
pub fn restage<L, A, B, V>(
    tree: &NodeTree<L, A>,
    visitor: &mut V,
) -> Result<NodeTree<L, B>, RestageError<V::Error>>
where
    L: Lang,
    V: RestageVisitor<L, A, B> + ?Sized,
{
    let mut cx = RestageContext {
        builder: NodeTreeBuilder::new(),
        replaced: HashMap::new(),
        _input: PhantomData,
    };
    let ids = drive(&mut cx, tree.root(), visitor)?;
    if ids.len() != 1 {
        return Err(RestageError::RootNotSingular { count: ids.len() });
    }
    cx.builder.finish(ids[0]).map_err(RestageError::Build)
}

/// Restage one subtree through the visitor: ask the verdict for `node`, then
/// either recurse over all children and restage the node over their results
/// (`Descend`), or accept the callback's staged replacement (`Emit`). Records
/// the node's replacement in the context's map either way.
pub(super) fn drive<L, A, B, V>(
    cx: &mut RestageContext<'_, L, A, B>,
    node: NodeRef<'_, L, A>,
    visitor: &mut V,
) -> Result<Vec<BuildId>, RestageError<V::Error>>
where
    L: Lang,
    V: RestageVisitor<L, A, B> + ?Sized,
{
    match visitor.restage(node, cx).map_err(RestageError::Visitor)? {
        Restage::Emit(ids) => {
            cx.record_emit(node.id(), &ids);
            Ok(ids)
        }
        Restage::Descend(annotation) => {
            // The safety invariant: Descend ALWAYS descends — every child
            // subtree goes through the visitor (structurally: slot children of
            // any role included).
            let mut replacements = Vec::with_capacity(node.child_count());
            for child in node.children() {
                replacements.push(drive(cx, child, visitor)?);
            }
            let id = cx.restage_over(node, &replacements, annotation)?;
            Ok(alloc::vec![id])
        }
    }
}

/// How one input node was restaged (the content-parent translation oracle).
#[derive(Clone, Debug)]
enum Replaced {
    /// Driver-restaged over its children's replacements: the staged id plus the
    /// replacement-length prefix sums over the old children — the table that
    /// translates an [`InChildrenOf`](crate::core::node::ContentNodes::InChildrenOf)
    /// content range through the parent's own restaging.
    Restaged { id: BuildId, prefix: Vec<u32> },
    /// An `Emit` takeover with exactly one staged node: content ranges into it
    /// are carried verbatim (the visitor chose the replacement's shape) and
    /// re-validated at staging.
    One(BuildId),
    /// An `Emit` takeover with zero or several staged nodes (the count) — no
    /// shape an `InChildrenOf` designation could re-anchor onto.
    Count(usize),
}

/// The staging side of a [`restage`] run, handed to every visitor call: the
/// region-aware restaging ops and the raw output
/// [`builder()`](RestageContext::builder) underneath them.
///
/// The ops accept nodes from **any** tree (the [module docs](super)' cross-tree
/// contract), and driving the same node more than once is legal — each drive
/// stages a fresh copy (the internal replacement map keeps the latest, which is
/// what subsequent content-parent translations resolve against).
pub struct RestageContext<'t, L: Lang, A, B> {
    builder: NodeTreeBuilder<L, B>,
    /// Input-node id → its staged replacement, recorded for every driven node;
    /// the `content_parents` oracle for record translation (tree-tagged ids, so
    /// entries from several input trees never collide).
    replaced: HashMap<NodeId, Replaced>,
    /// The run's frozen input tree, as a type/lifetime anchor: the context
    /// itself stores no borrow of it (ops take their input nodes explicitly and
    /// accept any tree's).
    _input: PhantomData<&'t NodeTree<L, A>>,
}

impl<'t, L: Lang, A, B> RestageContext<'t, L, A, B> {
    /// The raw staging builder of the output tree — arbitrary programmatic
    /// staging underneath the canned ops (they are conveniences, not the power
    /// boundary). Newly synthesized nodes are minted with the explicit two-line
    /// recipe: call
    /// [`Lang::make_node_ext`](crate::core::Lang::make_node_ext) (over
    /// [`staged_children`](NodeTreeBuilder::staged_children)), then
    /// [`add`](NodeTreeBuilder::add); restaged copies carry their old ext
    /// verbatim.
    pub fn builder(&mut self) -> &mut NodeTreeBuilder<L, B> {
        &mut self.builder
    }

    /// Record an `Emit` takeover's replacement for `old`.
    pub(super) fn record_emit(&mut self, old: NodeId, ids: &[BuildId]) {
        let entry = match ids {
            [one] => Replaced::One(*one),
            _ => Replaced::Count(ids.len()),
        };
        self.replaced.insert(old, entry);
    }

    /// Stage `node` over its children's replacements (the level-0 restage
    /// arithmetic with the run's replacement map as the content-parent oracle:
    /// content ranges into driver-restaged parents are *translated* through the
    /// parent's own replacements, ranges into single-node `Emit` replacements
    /// carried verbatim), record the result, and upgrade an unmapped content
    /// parent into the diagnosed
    /// [`ContentParentDropped`](RestageError::ContentParentDropped).
    pub(super) fn restage_over<AOld, E>(
        &mut self,
        node: NodeRef<'_, L, AOld>,
        replacements: &[Vec<BuildId>],
        annotation: B,
    ) -> Result<BuildId, RestageError<E>> {
        let replaced = &self.replaced;
        let result = self
            .builder
            .restage_node_with_content_mapping(
                node,
                replacements,
                |old| match replaced.get(&old) {
                    Some(Replaced::Restaged { id, prefix }) => {
                        Some(ContentParentMapping::Translate(*id, prefix))
                    }
                    Some(Replaced::One(id)) => Some(ContentParentMapping::Verbatim(*id)),
                    _ => None,
                },
                annotation,
            )
            .map_err(|error| match error {
                NodeBuildError::ContentParentUnmapped { parent } => {
                    RestageError::ContentParentDropped {
                        callable: node.id(),
                        parent,
                        replaced_by: match replaced.get(&parent) {
                            Some(Replaced::Restaged { .. }) | Some(Replaced::One(_)) => Some(1),
                            Some(Replaced::Count(count)) => Some(*count),
                            None => None,
                        },
                    }
                }
                other => RestageError::Build(other),
            })?;
        // Record the restage together with its replacement prefix sums — an
        // ancestor's record may designate content inside this node.
        let mut prefix: Vec<u32> = Vec::with_capacity(replacements.len() + 1);
        let mut total: u32 = 0;
        prefix.push(0);
        for entry in replacements {
            // In bounds: the staged add() above capped the total child count.
            total += entry.len() as u32;
            prefix.push(total);
        }
        self.replaced.insert(node.id(), Replaced::Restaged { id: result, prefix });
        Ok(result)
    }
}

impl<L: Lang, A, B> core::fmt::Debug for RestageContext<'_, L, A, B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RestageContext")
            .field("builder", &self.builder)
            .field("replaced", &self.replaced.len())
            .finish()
    }
}
