//! [`check_tree_invariants`]: the mechanical checker for the pinned whitespace/span
//! invariants (DESIGN_RATIONALE.md [§dd-dr:nodes]; ARCHITECTURE.md [§dd-arch:nodes]) — **a test utility,
//! deliberately not builder law**: a future construct that legitimately breaks byte
//! accounting (e.g. a tolerant root recovery that *skips* a stray close, leaving an
//! unrepresented byte) amends a test, not the architecture.

use alloc::vec;
use alloc::vec::Vec;
use core::ops::Range;

use crate::source::TextContent;
use crate::state::Lang;

use super::kind::NodeKind;
use super::tree::{NodeData, NodeTree};

/// Check a finished tree against the pinned structural and span invariants, panicking
/// with a description of the first violation. Intended for test suites (assert every
/// tree a parser produces) — run it liberally; it is O(n).
///
/// Checked:
///
/// 1. **Structural sanity.** Children ranges are in bounds and only ever *after* their
///    parent (the breadth-first layout); every non-root node is inside exactly one
///    parent's children range (single parent + reachability); the root is nobody's
///    child.
/// 2. **Partition of content interiors** ([§dd-dr:nodes] invariant 5). The sibling spans of a
///    `List`'s or `Group`'s children partition the parent's content interior exactly —
///    no gaps, no double counting. A `List`'s interior is its whole span; a `Group`'s
///    interior is its span minus the delimiters. `Chars` and `Comment` nodes are
///    childless.
/// 3. **Callable children-block contiguity.** A `Callable`'s children block is
///    span-contiguous (sibling spans chain without gaps) and lies inside the node's
///    span; a `Spanned` post-space — exactly the trigger token's own syntactic
///    post-space ([§dd-dr:nodes] invariant 3, as amended in Phase 6.4) — ends where the first
///    child begins, or ends the node's span when there are no children (the
///    argument-less shape, where it *is* trailing).
/// 4. **Region tiling.** A `Callable`'s resolved argument/slot regions tile its children
///    block exactly, in order (arguments before slots); every content range lies within
///    its content parent's children, and a content parent other than the callable itself
///    lies inside its own region's subtree.
/// 5. **`TextContent::Spanned` residency.** Every `Spanned` payload is a valid,
///    char-boundary range of the node's own source; where the payload has a pinned
///    position (chars content = the node's span; comment start/content/post-space
///    partition the node's span; group delimiters are prefix/suffix; callable post-space
///    is a suffix), the exact position is checked.
///
/// Byte-position comparisons require children to live in their parent's source; the
/// checker asserts that too (mixed-origin *transform* trees are a post-Phase-6 topic).
pub fn check_tree_invariants<L: Lang>(tree: &NodeTree<L>) {
    let n = tree.node_count();

    // --- 1. structural sanity ---------------------------------------------------------
    let mut parent: Vec<Option<u32>> = vec![None; n];
    for (i, data) in tree.nodes.iter().enumerate() {
        let children = &data.children;
        assert!(
            children.start <= children.end && (children.end as usize) <= n,
            "node {}: children range {:?} out of bounds ({} nodes)",
            i,
            children,
            n
        );
        assert!(
            children.is_empty() || (children.start as usize) > i,
            "node {}: children range {:?} does not lie after the node (breadth-first layout)",
            i,
            children
        );
        for c in children.clone() {
            assert!(
                parent[c as usize].is_none(),
                "node {} is inside two children ranges (parents {} and {})",
                c,
                parent[c as usize].unwrap(),
                i
            );
            parent[c as usize] = Some(i as u32);
        }
    }
    assert!(n > 0, "a tree has at least its root node");
    assert!(parent[0].is_none(), "the root is another node's child");
    for (i, p) in parent.iter().enumerate().skip(1) {
        assert!(p.is_some(), "node {} is unreachable from the root", i);
    }

    // --- 2..5. per-node span and payload checks ----------------------------------------
    for (i, data) in tree.nodes.iter().enumerate() {
        check_node(tree, &parent, i, data);
    }
}

fn check_node<L: Lang>(
    tree: &NodeTree<L>,
    parent: &[Option<u32>],
    i: usize,
    data: &NodeData<L>,
) {
    let span = data.span.range();
    let source_content = data.span.source().content();

    // `Spanned` residency: a valid char-boundary range of the node's own source.
    let residency = |text: &TextContent, what: &str| {
        if let TextContent::Spanned(s) = text {
            assert!(
                source_content.get(s.range()).is_some(),
                "node {}: {} span {:?} is not a valid range of the node's source (len {})",
                i,
                what,
                s,
                source_content.len()
            );
        }
    };
    // Resolved length of a payload (what interior arithmetic is based on).
    let text_len = |text: &TextContent| text.resolve(source_content).len();

    // The children of a node must live in its source for byte comparisons to mean
    // anything; check it wherever children exist.
    let assert_children_in_source = || {
        for child in tree.nodes_in(data.children.clone()) {
            assert!(
                child.span().same_source(&data.span),
                "node {}: child {} lives in a different source",
                i,
                child.id().index()
            );
        }
    };

    match &data.kind {
        NodeKind::Chars { content, .. } => {
            assert!(data.children.is_empty(), "node {}: a Chars node has children", i);
            residency(content, "chars content");
            if let TextContent::Spanned(s) = content {
                assert!(
                    s.range() == span,
                    "node {}: spanned chars content {:?} is not the node's span {:?}",
                    i,
                    s,
                    span
                );
            }
        }

        NodeKind::Comment { content, start, post_space, .. } => {
            assert!(data.children.is_empty(), "node {}: a Comment node has children", i);
            residency(content, "comment content");
            residency(start, "comment start delimiter");
            residency(post_space, "comment post-space");
            if let TextContent::Spanned(s) = start {
                assert!(
                    s.start() == span.start,
                    "node {}: spanned comment start {:?} does not begin the node's span {:?}",
                    i,
                    s,
                    span
                );
            }
            if let TextContent::Spanned(s) = post_space {
                assert!(
                    s.end() == span.end,
                    "node {}: spanned comment post-space {:?} does not end the node's span {:?}",
                    i,
                    s,
                    span
                );
            }
            if let (TextContent::Spanned(s), TextContent::Spanned(c), TextContent::Spanned(p)) =
                (start, content, post_space)
            {
                assert!(
                    s.end() == c.start() && c.end() == p.start(),
                    "node {}: comment parts {:?}/{:?}/{:?} do not partition the node's span {:?}",
                    i,
                    s,
                    c,
                    p,
                    span
                );
            }
        }

        NodeKind::List { .. } => {
            assert_children_in_source();
            check_interior_partition(tree, i, data, span.clone());
        }

        NodeKind::Group(group) => {
            residency(&group.open, "group open delimiter");
            residency(&group.close, "group close delimiter");
            let open_len = text_len(&group.open);
            let close_len = text_len(&group.close);
            assert!(
                open_len + close_len <= span.len(),
                "node {}: group delimiters (lengths {} + {}) exceed the node's span {:?}",
                i,
                open_len,
                close_len,
                span
            );
            if let TextContent::Spanned(s) = &group.open {
                assert!(
                    s.range() == (span.start..span.start + open_len),
                    "node {}: spanned open delimiter {:?} is not the prefix of span {:?}",
                    i,
                    s,
                    span
                );
            }
            if let TextContent::Spanned(s) = &group.close {
                assert!(
                    s.range() == (span.end - close_len..span.end),
                    "node {}: spanned close delimiter {:?} is not the suffix of span {:?}",
                    i,
                    s,
                    span
                );
            }
            assert_children_in_source();
            check_interior_partition(tree, i, data, span.start + open_len..span.end - close_len);
        }

        NodeKind::Callable(callable) => {
            residency(&callable.post_space, "callable post-space");
            if let TextContent::Spanned(s) = &callable.post_space {
                // The trigger token's own syntactic post-space ([§dd-dr:nodes] invariant 3 as
                // amended): between the name and the first child region — or trailing
                // when there are no children (revised in Phase 6.5; it asserted
                // "trailing" unconditionally while all callables were argument-less).
                let expected_end = tree
                    .nodes_in(data.children.clone())
                    .next()
                    .map(|first| first.span().start())
                    .unwrap_or(span.end);
                assert!(
                    s.end() == expected_end && s.start() >= span.start,
                    "node {}: spanned post-space {:?} does not end at the first child \
                     (or the span end {:?} for a childless callable)",
                    i,
                    s,
                    span
                );
            }
            assert_children_in_source();

            // Children-block span-contiguity, inside the node's span.
            let mut prev_end: Option<usize> = None;
            for child in tree.nodes_in(data.children.clone()) {
                let child_span = child.span().range();
                assert!(
                    span.start <= child_span.start && child_span.end <= span.end,
                    "node {}: child {} span {:?} escapes the callable's span {:?}",
                    i,
                    child.id().index(),
                    child_span,
                    span
                );
                if let Some(end) = prev_end {
                    assert!(
                        child_span.start == end,
                        "node {}: children block not span-contiguous at child {} \
                         (previous ends at {}, next starts at {})",
                        i,
                        child.id().index(),
                        end,
                        child_span.start
                    );
                }
                prev_end = Some(child_span.end);
            }

            check_regions(tree, parent, i, data, callable);
        }
    }
}

/// Invariant 2: the children of node `i` partition `interior` exactly.
fn check_interior_partition<L: Lang>(
    tree: &NodeTree<L>,
    i: usize,
    data: &NodeData<L>,
    interior: Range<usize>,
) {
    let mut pos = interior.start;
    for child in tree.nodes_in(data.children.clone()) {
        let child_span = child.span().range();
        assert!(
            child_span.start == pos,
            "node {}: gap {}..{} before child {} (partition invariant)",
            i,
            pos,
            child_span.start,
            child.id().index()
        );
        pos = child_span.end;
    }
    assert!(
        pos == interior.end,
        "node {}: children end at {} but the content interior ends at {} \
         (partition invariant)",
        i,
        pos,
        interior.end
    );
}

/// Invariant 4: the callable's resolved regions tile its children block; content ranges
/// sit inside their content parent, which sits inside its own region's subtree.
fn check_regions<L: Lang>(
    tree: &NodeTree<L>,
    parent: &[Option<u32>],
    i: usize,
    data: &NodeData<L>,
    callable: &super::kind::CallableData<L>,
) {
    let block = &data.children;
    let regions = callable
        .arguments
        .iter()
        .filter_map(|argument| argument.region.as_ref())
        .chain(callable.slots.iter().map(|slot| &slot.region));
    let mut next = block.start;
    for (r, region) in regions.enumerate() {
        assert!(
            region.is_resolved(),
            "node {}: region {} of a finished tree is still staged",
            i,
            r
        );
        let children = region.children();
        assert!(
            children.start == next && children.end <= block.end,
            "node {}: region {} children {:?} do not tile the children block {:?} \
             (expected to start at {})",
            i,
            r,
            children,
            block,
            next
        );
        next = children.end;

        let content = region.content_range();
        let content_parent = region.content_parent();
        if content_parent.index() == i {
            assert!(
                children.start <= content.start && content.end <= children.end,
                "node {}: region {} content {:?} escapes its region children {:?}",
                i,
                r,
                content,
                children
            );
        } else {
            let parent_children = &tree.nodes[content_parent.index()].children;
            assert!(
                parent_children.start <= content.start && content.end <= parent_children.end,
                "node {}: region {} content {:?} escapes its content parent's children {:?}",
                i,
                r,
                content,
                parent_children
            );
            // Walk up from the content parent to its child-of-the-callable ancestor;
            // that ancestor must be one of this region's nodes.
            let mut a = content_parent.index() as u32;
            loop {
                let p = parent[a as usize].unwrap_or_else(|| {
                    panic!(
                        "node {}: region {} content parent {} is not inside the \
                         callable's subtree",
                        i,
                        r,
                        content_parent.index()
                    )
                });
                if p == i as u32 {
                    break;
                }
                a = p;
            }
            assert!(
                children.contains(&a),
                "node {}: region {} content parent {} lies outside its own region {:?}",
                i,
                r,
                content_parent.index(),
                children
            );
        }
    }
    assert!(
        next == block.end,
        "node {}: regions end at {} but the children block is {:?} (regions must tile it)",
        i,
        next,
        block
    );
}

#[cfg(test)]
mod tests {
    use super::super::builder::NodeTreeBuilder;
    use super::super::kind::{GroupData, NodeKind};
    use super::*;
    use crate::scopes::ScopeStack;
    use crate::source::{Source, SourceSpan, Span};
    use crate::state::{ParsingState, SimpleLang, StateData};
    use crate::token::{TokenRules, WhitespaceRules};
    use alloc::sync::Arc;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl SimpleLang for PlainLang {}

    fn state() -> Arc<ParsingState<PlainLang>> {
        Arc::new(ParsingState::new(StateData {
            rules: TokenRules {
                enable_whitespace: true,
                whitespace: WhitespaceRules { chars: " \t\n".into() },
                enable_multi_newline_paragraphs: true,
                enable_groups: true,
                groups: alloc::vec::Vec::new(),
                temporary_groups: alloc::vec::Vec::new(),
                enable_commands: true,
                commands: alloc::vec::Vec::new(),
                enable_comments: true,
                comments: alloc::vec::Vec::new(),
                enable_specials: true,
                forbidden_chars: "".into(),
                expecting_group_close: None,
            },
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }))
    }

    /// `"a{bc}d"` hand-built correctly: root List [chars, group [chars], chars].
    fn build_valid() -> super::super::tree::NodeTree<PlainLang> {
        let source: Arc<Source> = Arc::new(Source::new("a{bc}d"));
        let st = state();
        let mut builder: NodeTreeBuilder<PlainLang> = NodeTreeBuilder::new();
        let a = builder.add(
            NodeKind::chars(Span::new(0, 1)),
            SourceSpan::new(&source, 0..1),
            Arc::clone(&st),
            alloc::vec![],
        ).unwrap();
        let bc = builder.add(
            NodeKind::chars(Span::new(2, 4)),
            SourceSpan::new(&source, 2..4),
            Arc::clone(&st),
            alloc::vec![],
        ).unwrap();
        let group = builder.add(
            NodeKind::group(GroupData::new(0u32, Span::new(1, 2), Span::new(4, 5))),
            SourceSpan::new(&source, 1..5),
            Arc::clone(&st),
            alloc::vec![bc],
        ).unwrap();
        let d = builder.add(
            NodeKind::chars(Span::new(5, 6)),
            SourceSpan::new(&source, 5..6),
            Arc::clone(&st),
            alloc::vec![],
        ).unwrap();
        let root = builder.add(
            NodeKind::list(),
            SourceSpan::entire(&source),
            Arc::clone(&st),
            alloc::vec![a, group, d],
        ).unwrap();
        builder.finish(root).unwrap()
    }

    #[test]
    fn accepts_a_valid_tree() {
        check_tree_invariants(&build_valid());
    }

    #[test]
    fn accepts_a_materialized_tree() {
        // All-owned payloads: positional checks are skipped, structure still verified.
        check_tree_invariants(&build_valid().materialize());
    }

    #[test]
    #[should_panic(expected = "partition invariant")]
    fn rejects_a_gap_between_siblings() {
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let st = state();
        let mut builder: NodeTreeBuilder<PlainLang> = NodeTreeBuilder::new();
        // Only "b" staged; the root's interior 0..2 has a gap at 0..1.
        let b = builder.add(
            NodeKind::chars(Span::new(1, 2)),
            SourceSpan::new(&source, 1..2),
            Arc::clone(&st),
            alloc::vec![],
        ).unwrap();
        let root = builder.add(
            NodeKind::list(),
            SourceSpan::entire(&source),
            Arc::clone(&st),
            alloc::vec![b],
        ).unwrap();
        check_tree_invariants(&builder.finish(root).unwrap());
    }

    #[test]
    #[should_panic(expected = "spanned chars content")]
    fn rejects_spanned_content_that_is_not_the_nodes_span() {
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let st = state();
        let mut builder: NodeTreeBuilder<PlainLang> = NodeTreeBuilder::new();
        // Content span 0..1 under a node span 0..2.
        let root = builder.add(
            NodeKind::chars(Span::new(0, 1)),
            SourceSpan::new(&source, 0..2),
            Arc::clone(&st),
            alloc::vec![],
        ).unwrap();
        check_tree_invariants(&builder.finish(root).unwrap());
    }

    #[test]
    #[should_panic(expected = "prefix of span")]
    fn rejects_a_group_whose_open_delimiter_is_misplaced() {
        let source: Arc<Source> = Arc::new(Source::new("x{}"));
        let st = state();
        let mut builder: NodeTreeBuilder<PlainLang> = NodeTreeBuilder::new();
        // Open delimiter recorded at 0..1 but the group's span starts at 1.
        let root = builder.add(
            NodeKind::group(GroupData::new(0u32, Span::new(0, 1), Span::new(2, 3))),
            SourceSpan::new(&source, 1..3),
            Arc::clone(&st),
            alloc::vec![],
        ).unwrap();
        check_tree_invariants(&builder.finish(root).unwrap());
    }
}
