//! Test helpers for tree serialization, reachable in-crate (`crate::serialize::tree_support`)
//! by the tree driver's own tests and, later, the preset's: a structural
//! [`assert_trees_equivalent`] deep-compare and a [`round_trip_tree`] harness that
//! serializes a tree into a fresh session, (under the `serde` feature) encodes the
//! segment to JSON and back, absorbs it into a second session, reads the tree back,
//! and deep-compares it against the original.

#![cfg(test)]

use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::Debug;

use crate::node::{NodeData, NodeKind, NodeTree};
use crate::state::{InvocationSyntax, Lang};

use super::{
    DeserializableValue, SerdeSession, SerialIndex, SerializableLang, SerializableValue,
    TreeSerialization,
};

/// Assert that two trees are structurally equivalent: same node count, and, in
/// storage order, the same kind and payload (the resolved text of every content, the
/// group and callable types, names, region ranges, slot roles, the debug forms of
/// specs and exts, and the debug forms of the invocation syntaxes materialized against
/// their node's source), the same spans (offsets and source text),
/// the same parsing states (debug forms), the same identity topology (the
/// `Arc::ptr_eq` classes of states, specs, and span sources), the same parent tables,
/// the same single-source flag, and, via `annotation_eq`, the same annotations.
///
/// Panics on the first difference with a message naming the node.
pub(crate) fn assert_trees_equivalent<L: Lang, A>(
    left: &NodeTree<L, A>,
    right: &NodeTree<L, A>,
    annotation_eq: impl Fn(&A, &A) -> bool,
) {
    let (left_nodes, right_nodes) = (left.nodes(), right.nodes());
    assert_eq!(left_nodes.len(), right_nodes.len(), "node count differs");

    for (index, (a, b)) in left_nodes.iter().zip(right_nodes.iter()).enumerate() {
        assert_node_equivalent(index, a, b);
        assert!(
            annotation_eq(&left.annotations()[index], &right.annotations()[index]),
            "node #{index}: annotations differ"
        );
    }

    // Identity topology: the `Arc::ptr_eq` classes must match between the trees.
    assert_eq!(
        class_vector(&pointers(left_nodes, node_state_ptr)),
        class_vector(&pointers(right_nodes, node_state_ptr)),
        "the shared-state topology differs"
    );
    assert_eq!(
        class_vector(&pointers(left_nodes, node_source_ptr)),
        class_vector(&pointers(right_nodes, node_source_ptr)),
        "the shared-source topology differs"
    );
    assert_eq!(
        class_vector(&pointers(left_nodes, node_spec_ptr)),
        class_vector(&pointers(right_nodes, node_spec_ptr)),
        "the shared-spec topology differs"
    );

    assert_eq!(left.parent_table(), right.parent_table(), "parent tables differ");
    assert_eq!(left.is_single_source(), right.is_single_source(), "single-source flags differ");
}

fn assert_node_equivalent<L: Lang>(index: usize, a: &NodeData<L>, b: &NodeData<L>) {
    assert_eq!(a.kind.as_str(), b.kind.as_str(), "node #{index}: kinds differ");
    assert_eq!(
        (a.span.start(), a.span.end()),
        (b.span.start(), b.span.end()),
        "node #{index}: span offsets differ"
    );
    assert_eq!(
        a.span.source().content(),
        b.span.source().content(),
        "node #{index}: span source text differs"
    );
    assert_eq!(
        format!("{:?}", a.parsing_state),
        format!("{:?}", b.parsing_state),
        "node #{index}: parsing states differ"
    );
    assert_eq!(a.children, b.children, "node #{index}: children ranges differ");

    match (&a.kind, &b.kind) {
        (NodeKind::Chars { content: ca }, NodeKind::Chars { content: cb }) => {
            assert_eq!(ca.resolve(a.span.source()), cb.resolve(b.span.source()), "node #{index}: chars differ");
        }
        (NodeKind::Group(ga), NodeKind::Group(gb)) => {
            assert_eq!(format!("{:?}", ga.group_type), format!("{:?}", gb.group_type), "node #{index}: group type");
            assert_eq!(ga.open.resolve(a.span.source()), gb.open.resolve(b.span.source()), "node #{index}: open");
            assert_eq!(ga.close.resolve(a.span.source()), gb.close.resolve(b.span.source()), "node #{index}: close");
        }
        (NodeKind::Comment(ca), NodeKind::Comment(cb)) => {
            assert_eq!(ca.start.resolve(a.span.source()), cb.start.resolve(b.span.source()), "node #{index}: c.start");
            assert_eq!(
                ca.content.resolve(a.span.source()),
                cb.content.resolve(b.span.source()),
                "node #{index}: c.content"
            );
            assert_eq!(
                ca.post_space.resolve(a.span.source()),
                cb.post_space.resolve(b.span.source()),
                "node #{index}: c.post_space"
            );
        }
        (NodeKind::List, NodeKind::List) => {}
        (NodeKind::Callable(ca), NodeKind::Callable(cb)) => {
            assert_eq!(format!("{:?}", ca.callable_type), format!("{:?}", cb.callable_type), "node #{index}: ct");
            assert_eq!(&*ca.name, &*cb.name, "node #{index}: callable name");
            assert_eq!(format!("{:?}", ca.spec), format!("{:?}", cb.spec), "node #{index}: spec");
            // Compared materialized: the tree writer materializes the invocation syntax
            // against the node's source (text inside language payloads is owned on the
            // wire), so a span-backed field reads back as the same text, owned.
            assert_eq!(
                format!("{:?}", ca.invocation_syntax.materialized(a.span.source())),
                format!("{:?}", cb.invocation_syntax.materialized(b.span.source())),
                "node #{index}: invocation syntax"
            );
            assert_eq!(ca.arguments.len(), cb.arguments.len(), "node #{index}: argument count");
            for (arg_index, (aa, ab)) in ca.arguments.iter().zip(cb.arguments.iter()).enumerate() {
                assert_eq!(aa.name(), ab.name(), "node #{index}: argument #{arg_index} name");
                assert_eq!(aa.is_provided(), ab.is_provided(), "node #{index}: argument #{arg_index} presence");
                match (&aa.region, &ab.region) {
                    (Some(ra), Some(rb)) => assert_region(index, ra, rb),
                    (None, None) => {}
                    _ => panic!("node #{index}: argument #{arg_index} region presence differs"),
                }
                assert_eq!(
                    format!("{:?}", aa.ext),
                    format!("{:?}", ab.ext),
                    "node #{index}: argument #{arg_index} ext"
                );
            }
            assert_eq!(ca.slots.len(), cb.slots.len(), "node #{index}: slot count");
            for (slot_index, (sa, sb)) in ca.slots.iter().zip(cb.slots.iter()).enumerate() {
                assert_eq!(sa.name(), sb.name(), "node #{index}: slot #{slot_index} name");
                assert_eq!(sa.role, sb.role, "node #{index}: slot #{slot_index} role");
                assert_region(index, &sa.region, &sb.region);
                assert_eq!(
                    format!("{:?}", sa.ext),
                    format!("{:?}", sb.ext),
                    "node #{index}: slot #{slot_index} ext"
                );
            }
        }
        _ => unreachable!("kinds compared above"),
    }
}

fn assert_region(index: usize, a: &crate::node::ChildRegion, b: &crate::node::ChildRegion) {
    assert_eq!(a.children(), b.children(), "node #{index}: region children");
    assert_eq!(a.content_range(), b.content_range(), "node #{index}: region content");
    assert_eq!(
        a.content_parent().index(),
        b.content_parent().index(),
        "node #{index}: region content parent"
    );
}

fn node_state_ptr<L: Lang>(node: &NodeData<L>) -> Option<usize> {
    Some(Arc::as_ptr(&node.parsing_state) as *const () as usize)
}

fn node_source_ptr<L: Lang>(node: &NodeData<L>) -> Option<usize> {
    Some(Arc::as_ptr(node.span.source()) as *const () as usize)
}

fn node_spec_ptr<L: Lang>(node: &NodeData<L>) -> Option<usize> {
    match &node.kind {
        NodeKind::Callable(data) => Some(Arc::as_ptr(&data.spec) as *const () as usize),
        _ => None,
    }
}

fn pointers<L: Lang>(nodes: &[NodeData<L>], get: impl Fn(&NodeData<L>) -> Option<usize>) -> Vec<Option<usize>> {
    nodes.iter().map(get).collect()
}

/// The equivalence classes of `ptrs`: each entry maps to the index of the first entry
/// with the same value (equal pointers, `None` with `None`).
fn class_vector(ptrs: &[Option<usize>]) -> Vec<usize> {
    let mut classes = Vec::with_capacity(ptrs.len());
    for (i, p) in ptrs.iter().enumerate() {
        let class = (0..i).find(|&j| ptrs[j] == *p).map_or(i, |j| classes[j]);
        classes.push(class);
    }
    classes
}

/// Serialize `tree` into a fresh session (built by `setup`), pass the segment through
/// a round trip (JSON under the `serde` feature), absorb it into a second session, read
/// the tree back, and deep-compare it against `tree`. Returns the rebuilt tree. `setup`
/// builds a session with the tree's tables, readers, and annotation codec registered.
pub(crate) fn round_trip_tree<L, A>(
    setup: impl Fn() -> SerdeSession<L>,
    tree: &NodeTree<L, A>,
    annotation_eq: impl Fn(&A, &A) -> bool,
) -> NodeTree<L, A>
where
    L: SerializableLang,
    A: SerializableValue<L> + DeserializableValue<L> + Clone + Debug + Send + Sync + 'static,
{
    let mut writer = setup();
    let position = writer.serialize_tree(tree).unwrap();
    let segment = writer.take_segment();

    #[cfg(feature = "serde")]
    let segment = {
        let json = serde_json::to_string(&segment).expect("segment to JSON");
        serde_json::from_str(&json).expect("segment from JSON")
    };

    let mut reader = setup();
    reader.push_segment(segment).unwrap();
    let trees = reader.table_handle::<super::TreeSerdeDriver<L>>("trees").expect("the trees table");
    let back: NodeTree<L, A> = reader.tree(trees.position(position.index())).unwrap();
    assert_trees_equivalent(tree, &back, &annotation_eq);
    back
}

/// A convenience for annotation types with no identity to compare (`()`): every pair
/// is equal.
pub(crate) fn ignore_annotations<A>(_a: &A, _b: &A) -> bool {
    true
}
