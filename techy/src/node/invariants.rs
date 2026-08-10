//! Tree validation: [`validate_tree`] checks the **all-trees law** — what every
//! finished tree must satisfy regardless of origin — as a `Result`; the crate-internal
//! `check_tree_invariants` (test builds only) is the panicking test-suite oracle for
//! the stricter **parse-tree law** (byte accounting), layered on top of it.
//!
//! The split is deliberate: legitimate transform output (spliced, reordered,
//! synthesized nodes) breaks the parse-tree law's byte accounting *by design*, so the
//! byte checks are **a test utility, not builder law** — a future construct that
//! legitimately breaks byte accounting (e.g. a tolerant root recovery that *skips* a
//! stray close, leaving an unrepresented byte) amends a test, not the architecture.
//! The all-trees law is a subset of the parse-tree law.

use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use crate::source::{Span, TextContent};
use crate::state::Lang;

use super::kind::NodeKind;
use super::tree::{NodeData, NodeId, NodeTree};

/// Check a tree against the **all-trees law** — the invariants every finished
/// [`NodeTree`] satisfies regardless of how it was built (parsed, restaged, spliced
/// across trees, synthesized):
///
/// 1. **Structural sanity.** Children ranges are in bounds and only ever *after*
///    their node (the breadth-first layout); every non-root node is inside exactly
///    one parent's children range (single parent + reachability); the root is
///    nobody's child; `Chars` and `Comment` nodes are childless.
/// 2. **Region tiling.** A `Callable`'s argument/slot regions are resolved and tile
///    its children block exactly, in order (arguments before slots, independent of
///    each slot's [`SlotRole`](super::SlotRole)); every content range lies within its
///    content parent's children, and a content parent other than the callable itself
///    lies inside its own region's subtree.
/// 3. **`TextContent::Spanned` residency.** Every `Spanned` payload is a valid,
///    char-boundary range of the node's own source — residency only, no positional
///    pinning.
///
/// Deliberately **not** checked — the parse-tree law's byte accounting, which
/// legitimate transform output breaks by design: no byte partition of content
/// interiors, no children-share-parent's-source, no sibling source order, no
/// positional payload pins (chars content = the node's span, delimiter
/// prefix/suffix positions, …). A multi-source tree (attached `\input` content, a
/// cross-tree splice) passes this function.
///
/// Returns the first violation found as an `Err` — this function **never panics**:
/// it is the runtime validator for frameworks accepting rebuilt or spliced trees
/// (FFI included), not a test assertion. O(n) in the number of nodes.
pub fn validate_tree<L: Lang, A>(tree: &NodeTree<L, A>) -> Result<(), TreeViolation> {
    let n = tree.node_count();
    if n == 0 {
        // Unreachable for trees built by `NodeTreeBuilder::finish` (which always
        // stages a root); kept so the checker is total.
        return Err(TreeViolation { node: None, kind: TreeViolationKind::Empty });
    }

    // --- 1. structural sanity ---------------------------------------------------------
    // The parent map is *recomputed* from the children ranges: the tree's stored parent
    // table is derived data, and re-deriving the relation here is the check itself.
    let mut parent: Vec<Option<u32>> = vec![None; n];
    for (i, data) in tree.nodes().iter().enumerate() {
        let children = data.children.clone();
        if children.start > children.end || children.end as usize > n {
            return Err(TreeViolation {
                node: Some(tree.make_id(i as u32)),
                kind: TreeViolationKind::ChildrenOutOfBounds { children, node_count: n },
            });
        }
        if !children.is_empty() && children.start as usize <= i {
            return Err(TreeViolation {
                node: Some(tree.make_id(i as u32)),
                kind: TreeViolationKind::ChildrenNotAfterParent { children },
            });
        }
        for c in children {
            if let Some(first) = parent[c as usize] {
                return Err(TreeViolation {
                    node: Some(tree.make_id(c)),
                    kind: TreeViolationKind::MultipleParents {
                        first_parent: tree.make_id(first),
                        second_parent: tree.make_id(i as u32),
                    },
                });
            }
            parent[c as usize] = Some(i as u32);
        }
    }
    if let Some(p) = parent[0] {
        return Err(TreeViolation {
            node: Some(tree.make_id(0)),
            kind: TreeViolationKind::RootHasParent { parent: tree.make_id(p) },
        });
    }
    for (i, p) in parent.iter().enumerate().skip(1) {
        if p.is_none() {
            return Err(TreeViolation {
                node: Some(tree.make_id(i as u32)),
                kind: TreeViolationKind::Unreachable,
            });
        }
    }

    // --- 2. + 3. per-node record and payload checks ------------------------------------
    for (i, data) in tree.nodes().iter().enumerate() {
        validate_node(tree, &parent, i, data)?;
    }
    Ok(())
}

fn validate_node<L: Lang, A>(
    tree: &NodeTree<L, A>,
    parent: &[Option<u32>],
    i: usize,
    data: &NodeData<L>,
) -> Result<(), TreeViolation> {
    let source_content = data.span.source().content();

    // `Spanned` residency: a valid char-boundary range of the node's own source.
    let residency = |text: &TextContent, what: &'static str| -> Result<(), TreeViolation> {
        if let TextContent::Spanned(s) = text {
            if source_content.get(s.range()).is_none() {
                return Err(TreeViolation {
                    node: Some(tree.make_id(i as u32)),
                    kind: TreeViolationKind::SpannedContentInvalid {
                        what,
                        span: *s,
                        content_len: source_content.len(),
                    },
                });
            }
        }
        Ok(())
    };
    let leaf_without_children = |kind: &'static str| -> Result<(), TreeViolation> {
        if !data.children.is_empty() {
            return Err(TreeViolation {
                node: Some(tree.make_id(i as u32)),
                kind: TreeViolationKind::LeafWithChildren { kind },
            });
        }
        Ok(())
    };

    match &data.kind {
        NodeKind::Chars { content } => {
            leaf_without_children("Chars")?;
            residency(content, "chars content")
        }
        NodeKind::Comment { content, start, post_space } => {
            leaf_without_children("Comment")?;
            residency(content, "comment content")?;
            residency(start, "comment start delimiter")?;
            residency(post_space, "comment post-space")
        }
        NodeKind::List => Ok(()),
        NodeKind::Group(group) => {
            residency(&group.open, "group open delimiter")?;
            residency(&group.close, "group close delimiter")
        }
        NodeKind::Callable(callable) => {
            // The invocation-syntax payload is Lang-opaque to the all-trees law
            // (its span-backed fields are the Lang's recording discipline; the
            // in-crate parse-law oracle checks the shipped payloads).
            validate_regions(tree, parent, i, data, callable)
        }
    }
}

/// Region tiling (all-trees law, point 2): the callable's resolved regions tile its
/// children block; content ranges sit inside their content parent, which sits inside
/// its own region's subtree.
fn validate_regions<L: Lang, A>(
    tree: &NodeTree<L, A>,
    parent: &[Option<u32>],
    i: usize,
    data: &NodeData<L>,
    callable: &super::kind::CallableData<L>,
) -> Result<(), TreeViolation> {
    let node = tree.make_id(i as u32);
    let block = &data.children;
    let regions = callable
        .arguments
        .iter()
        .filter_map(|argument| argument.region.as_ref())
        .chain(callable.slots.iter().map(|slot| &slot.region));
    let mut next = block.start;
    for (r, region) in regions.enumerate() {
        if !region.is_resolved() {
            return Err(TreeViolation {
                node: Some(node),
                kind: TreeViolationKind::UnresolvedRegion { region: r },
            });
        }
        let children = region.children();
        if children.start != next || children.end > block.end || children.start > children.end
        {
            return Err(TreeViolation {
                node: Some(node),
                kind: TreeViolationKind::RegionNotTiling {
                    region: r,
                    children,
                    block: block.clone(),
                    expected_start: next,
                },
            });
        }
        next = children.end;

        let content = region.content_range();
        let content_parent = region.content_parent();
        if content_parent.index() == i {
            if content.start < children.start || content.end > children.end {
                return Err(TreeViolation {
                    node: Some(node),
                    kind: TreeViolationKind::ContentOutsideRegion {
                        region: r,
                        content,
                        region_children: children,
                    },
                });
            }
        } else {
            let Some(parent_data) = tree.nodes().get(content_parent.index()) else {
                // Not a node of this tree at all — certainly not inside the subtree.
                return Err(TreeViolation {
                    node: Some(node),
                    kind: TreeViolationKind::ContentParentOutsideSubtree {
                        region: r,
                        content_parent,
                    },
                });
            };
            let parent_children = parent_data.children.clone();
            if content.start < parent_children.start || content.end > parent_children.end {
                return Err(TreeViolation {
                    node: Some(node),
                    kind: TreeViolationKind::ContentOutsideContentParent {
                        region: r,
                        content,
                        parent_children,
                    },
                });
            }
            // Walk up from the content parent to its child-of-the-callable ancestor;
            // that ancestor must be one of this region's nodes. Terminates: the
            // structural pass verified children lie strictly after their parent, so
            // each step strictly decreases the index.
            let mut a = content_parent.index() as u32;
            loop {
                let Some(p) = parent[a as usize] else {
                    return Err(TreeViolation {
                        node: Some(node),
                        kind: TreeViolationKind::ContentParentOutsideSubtree {
                            region: r,
                            content_parent,
                        },
                    });
                };
                if p == i as u32 {
                    break;
                }
                a = p;
            }
            if !children.contains(&a) {
                return Err(TreeViolation {
                    node: Some(node),
                    kind: TreeViolationKind::ContentParentOutsideRegion {
                        region: r,
                        content_parent,
                        region_children: children,
                    },
                });
            }
        }
    }
    if next != block.end {
        return Err(TreeViolation {
            node: Some(node),
            kind: TreeViolationKind::ChildrenNotInRegions { unassigned: next..block.end },
        });
    }
    Ok(())
}

/// A violation of the all-trees law, reported by [`validate_tree`].
///
/// Carries the offending node (where one is identifiable) and the violation detail —
/// rich enough that a `Display` rendering serves directly as a panic or log message.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TreeViolation {
    /// The node the violation is about (e.g. the doubly-parented node itself for
    /// [`MultipleParents`](TreeViolationKind::MultipleParents), the callable for
    /// region violations); `None` when no single node is identifiable.
    pub node: Option<NodeId>,
    /// What was violated.
    pub kind: TreeViolationKind,
}

/// The specific all-trees-law violation inside a [`TreeViolation`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TreeViolationKind {
    /// The tree stores no nodes (a tree has at least its root node). Unreachable for
    /// trees built by [`NodeTreeBuilder::finish`](super::NodeTreeBuilder::finish);
    /// kept so the checker is total.
    Empty,
    /// A children range is inverted or reaches past the node storage.
    ChildrenOutOfBounds {
        /// The offending children range.
        children: Range<u32>,
        /// The tree's node count.
        node_count: usize,
    },
    /// A non-empty children range does not lie strictly after its node (the
    /// breadth-first layout law).
    ChildrenNotAfterParent {
        /// The offending children range.
        children: Range<u32>,
    },
    /// The node lies inside two parents' children ranges.
    MultipleParents {
        /// The first claiming parent (in storage order).
        first_parent: NodeId,
        /// The second claiming parent.
        second_parent: NodeId,
    },
    /// The root (index 0) lies inside another node's children range.
    RootHasParent {
        /// The claiming parent.
        parent: NodeId,
    },
    /// The node is inside no parent's children range (unreachable from the root).
    Unreachable,
    /// A `Chars` or `Comment` node has children.
    LeafWithChildren {
        /// The childless kind (`"Chars"` or `"Comment"`).
        kind: &'static str,
    },
    /// An argument/slot region of a finished tree is still staged (never resolved by
    /// [`NodeTreeBuilder::finish`](super::NodeTreeBuilder::finish)).
    UnresolvedRegion {
        /// The region's ordinal within the callable (arguments before slots).
        region: usize,
    },
    /// An argument/slot region does not start where the previous one ended, or
    /// escapes the callable's children block: the regions must tile the block
    /// exactly, in order.
    RegionNotTiling {
        /// The region's ordinal within the callable.
        region: usize,
        /// The offending region's node-index range.
        children: Range<u32>,
        /// The callable's children block.
        block: Range<u32>,
        /// Where the region was expected to start.
        expected_start: u32,
    },
    /// Children after the last argument/slot region belong to no region (the regions
    /// must tile the children block exactly).
    ChildrenNotInRegions {
        /// The node indices no region accounts for.
        unassigned: Range<u32>,
    },
    /// A region-level content range escapes its own region's nodes.
    ContentOutsideRegion {
        /// The region's ordinal within the callable.
        region: usize,
        /// The offending content range.
        content: Range<u32>,
        /// The region's node-index range.
        region_children: Range<u32>,
    },
    /// A content range escapes its content parent's children.
    ContentOutsideContentParent {
        /// The region's ordinal within the callable.
        region: usize,
        /// The offending content range.
        content: Range<u32>,
        /// The content parent's children range.
        parent_children: Range<u32>,
    },
    /// A content parent is not inside its callable's subtree (also reported when the
    /// recorded content parent is not a node of this tree at all).
    ContentParentOutsideSubtree {
        /// The region's ordinal within the callable.
        region: usize,
        /// The recorded content parent.
        content_parent: NodeId,
    },
    /// A content parent lies outside its own argument/slot region's subtree.
    ContentParentOutsideRegion {
        /// The region's ordinal within the callable.
        region: usize,
        /// The recorded content parent.
        content_parent: NodeId,
        /// The region's node-index range.
        region_children: Range<u32>,
    },
    /// A `Spanned` text payload is not a valid char-boundary range of the node's own
    /// source.
    SpannedContentInvalid {
        /// Which payload (e.g. `"chars content"`, `"group open delimiter"`).
        what: &'static str,
        /// The offending span.
        span: Span,
        /// The node's source content length.
        content_len: usize,
    },
}

impl TreeViolationKind {
    /// The variant's static name — the bare name without the variant's data
    /// (`"Empty"`, `"ChildrenOutOfBounds"`, …), following
    /// [`NodeKind::as_str`](super::NodeKind::as_str): for log labels and name-keyed
    /// tables that must not fall out of step when a variant is added or renamed
    /// (the enum is `#[non_exhaustive]`). The full detail is the
    /// [`Display`](core::fmt::Display) rendering; the data is on the variants
    /// themselves.
    pub const fn as_str(&self) -> &'static str {
        match self {
            TreeViolationKind::Empty => "Empty",
            TreeViolationKind::ChildrenOutOfBounds { .. } => "ChildrenOutOfBounds",
            TreeViolationKind::ChildrenNotAfterParent { .. } => "ChildrenNotAfterParent",
            TreeViolationKind::MultipleParents { .. } => "MultipleParents",
            TreeViolationKind::RootHasParent { .. } => "RootHasParent",
            TreeViolationKind::Unreachable => "Unreachable",
            TreeViolationKind::LeafWithChildren { .. } => "LeafWithChildren",
            TreeViolationKind::UnresolvedRegion { .. } => "UnresolvedRegion",
            TreeViolationKind::RegionNotTiling { .. } => "RegionNotTiling",
            TreeViolationKind::ChildrenNotInRegions { .. } => "ChildrenNotInRegions",
            TreeViolationKind::ContentOutsideRegion { .. } => "ContentOutsideRegion",
            TreeViolationKind::ContentOutsideContentParent { .. } => {
                "ContentOutsideContentParent"
            }
            TreeViolationKind::ContentParentOutsideSubtree { .. } => {
                "ContentParentOutsideSubtree"
            }
            TreeViolationKind::ContentParentOutsideRegion { .. } => {
                "ContentParentOutsideRegion"
            }
            TreeViolationKind::SpannedContentInvalid { .. } => "SpannedContentInvalid",
        }
    }
}

impl fmt::Display for TreeViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.node {
            Some(id) => write!(f, "node {}: {}", id.index(), self.kind),
            None => fmt::Display::fmt(&self.kind, f),
        }
    }
}

impl fmt::Display for TreeViolationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TreeViolationKind::Empty => {
                write!(f, "the tree has no nodes (a tree has at least its root node)")
            }
            TreeViolationKind::ChildrenOutOfBounds { children, node_count } => {
                write!(f, "children range {:?} out of bounds ({} nodes)", children, node_count)
            }
            TreeViolationKind::ChildrenNotAfterParent { children } => write!(
                f,
                "children range {:?} does not lie after the node (breadth-first layout)",
                children
            ),
            TreeViolationKind::MultipleParents { first_parent, second_parent } => write!(
                f,
                "the node is inside two children ranges (parents {:?} and {:?})",
                first_parent, second_parent
            ),
            TreeViolationKind::RootHasParent { parent } => {
                write!(f, "the root is another node's child (parent {:?})", parent)
            }
            TreeViolationKind::Unreachable => {
                write!(f, "the node is unreachable from the root")
            }
            TreeViolationKind::LeafWithChildren { kind } => {
                write!(f, "a {} node has children", kind)
            }
            TreeViolationKind::UnresolvedRegion { region } => {
                write!(f, "region {} of a finished tree is still staged", region)
            }
            TreeViolationKind::RegionNotTiling { region, children, block, expected_start } => {
                write!(
                    f,
                    "region {} children {:?} do not tile the children block {:?} \
                     (expected to start at {})",
                    region, children, block, expected_start
                )
            }
            TreeViolationKind::ChildrenNotInRegions { unassigned } => write!(
                f,
                "children {:?} belong to no argument/slot region \
                 (the regions must tile the children block)",
                unassigned
            ),
            TreeViolationKind::ContentOutsideRegion { region, content, region_children } => {
                write!(
                    f,
                    "region {} content {:?} escapes its region children {:?}",
                    region, content, region_children
                )
            }
            TreeViolationKind::ContentOutsideContentParent {
                region,
                content,
                parent_children,
            } => write!(
                f,
                "region {} content {:?} escapes its content parent's children {:?}",
                region, content, parent_children
            ),
            TreeViolationKind::ContentParentOutsideSubtree { region, content_parent } => {
                write!(
                    f,
                    "region {} content parent {:?} is not inside the callable's subtree",
                    region, content_parent
                )
            }
            TreeViolationKind::ContentParentOutsideRegion {
                region,
                content_parent,
                region_children,
            } => write!(
                f,
                "region {} content parent {:?} lies outside its own region {:?}",
                region, content_parent, region_children
            ),
            TreeViolationKind::SpannedContentInvalid { what, span, content_len } => write!(
                f,
                "{} span {:?} is not a valid char-boundary range of the node's source \
                 (len {})",
                what, span, content_len
            ),
        }
    }
}

impl core::error::Error for TreeViolation {}

/// Check a finished tree against the **parse-tree law** — the all-trees law
/// ([`validate_tree`]) plus the byte accounting that holds for parser output —
/// panicking with a description of the first violation. The in-crate test oracle
/// (assert every tree a parser produces); run it liberally, it is O(n).
///
/// On top of the all-trees law, this checks:
///
/// 1. **Partition of content interiors.** The sibling spans of a `List`'s or
///    `Group`'s children partition the parent's content interior exactly — no gaps,
///    no double counting. A `List`'s interior is its whole span; a `Group`'s interior
///    is its span minus the delimiters.
/// 2. **Callable children-block contiguity.** A `Callable`'s children block is
///    span-contiguous and lies inside the node's span.
/// 3. **Positional payload pins.** Where a `Spanned` payload has a pinned position
///    (chars content = the node's span; comment start/content/post-space partition
///    the node's span; group delimiters are prefix/suffix), the exact position is
///    checked. The **invocation-syntax payload is deliberately not read here**:
///    it is Lang-owned opaque data to core — the latexlike preset layers its
///    payload pins in its own checker (`check_latexlike_tree_invariants`, the
///    D-plan-12 Option B split).
/// 4. **Children share the parent's source** (byte comparisons presuppose it).
#[cfg(test)]
pub(crate) fn check_tree_invariants<L: Lang, A>(tree: &NodeTree<L, A>) {
    if let Err(violation) = validate_tree(tree) {
        panic!("tree invariant violated: {}", violation);
    }
    for (i, data) in tree.nodes().iter().enumerate() {
        check_parse_law_node(tree, i, data);
    }
}

/// The parse-law byte accounting for one node (see [`check_tree_invariants`];
/// the all-trees law has already passed).
///
/// Byte accounting is scoped **per source** via the slot roles
/// ([`SlotRole`](super::SlotRole)): children in an `Attached` slot's region live
/// in the attached source (`\input`) and are excluded from the including
/// callable's children-in-source/contiguity checks — with their own per-source
/// accounting (one source per attached region, span-contiguous within it) —
/// while children in a `Hidden` slot's region carry **no** byte accounting at
/// all (the ruled `Hidden` semantics). Declaration replaces source-change
/// inference: the excluded regions are invisible to the parent's accounting,
/// so the remaining children must be contiguous across them.
#[cfg(test)]
fn check_parse_law_node<L: Lang, A>(tree: &NodeTree<L, A>, i: usize, data: &NodeData<L>) {
    let span = data.span.range();
    let source = data.span.source();

    // Resolved length of a payload (what interior arithmetic is based on).
    let text_len = |text: &TextContent| text.resolve(source).len();

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
        NodeKind::Chars { content } => {
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

        NodeKind::Comment { content, start, post_space } => {
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

        NodeKind::List => {
            assert_children_in_source();
            check_interior_partition(tree, i, data, span.clone());
        }

        NodeKind::Group(group) => {
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
            // The trigger-spelling facts live in the Lang-owned invocation-syntax
            // payload (invariant 3 as amended) — opaque to core, so no payload
            // byte accounting happens here: the latexlike preset pins its
            // recorded spellings (macro spelling + post-space, specials
            // name-as-written, environment begin/end scaffolding) in its own
            // checker, `check_latexlike_tree_invariants` (D-plan-12 Option B).

            // Partition the children block by slot role (see the fn docs):
            // `Attached` regions get their own per-source accounting below,
            // `Hidden` regions get none, the rest is the parent's accounting.
            use super::SlotRole;
            let attached_regions: Vec<Range<u32>> = callable
                .slots
                .iter()
                .filter(|slot| slot.role == SlotRole::Attached)
                .map(|slot| slot.region.children())
                .collect();
            let hidden_regions: Vec<Range<u32>> = callable
                .slots
                .iter()
                .filter(|slot| slot.role == SlotRole::Hidden)
                .map(|slot| slot.region.children())
                .collect();
            let excluded = |index: u32| {
                attached_regions
                    .iter()
                    .chain(hidden_regions.iter())
                    .any(|region| region.contains(&index))
            };

            // The parent-source sequence (arguments + `Content` slots):
            // children share the parent's source, lie inside the node's span,
            // and are span-contiguous *across* the excluded regions.
            let mut prev_end: Option<usize> = None;
            for child in tree.nodes_in(data.children.clone()) {
                if excluded(child.id().index() as u32) {
                    continue;
                }
                assert!(
                    child.span().same_source(&data.span),
                    "node {}: child {} lives in a different source",
                    i,
                    child.id().index()
                );
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

            // Each `Attached` region's own per-source accounting: one source
            // per region (the attached source), span-contiguous within it.
            for region in &attached_regions {
                let mut prev: Option<super::NodeRef<'_, L, A>> = None;
                for child in tree.nodes_in(region.clone()) {
                    if let Some(prev) = &prev {
                        assert!(
                            child.span().same_source(prev.span()),
                            "node {}: attached region {:?} spans several sources \
                             (children {} and {})",
                            i,
                            region,
                            prev.id().index(),
                            child.id().index()
                        );
                        assert!(
                            child.span().range().start == prev.span().range().end,
                            "node {}: attached region {:?} not span-contiguous at \
                             child {} (previous ends at {}, next starts at {})",
                            i,
                            region,
                            child.id().index(),
                            prev.span().range().end,
                            child.span().range().start
                        );
                    }
                    prev = Some(child);
                }
            }
        }
    }
}

/// Parse-law point 1: the children of node `i` partition `interior` exactly.
#[cfg(test)]
fn check_interior_partition<L: Lang, A>(
    tree: &NodeTree<L, A>,
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

#[cfg(test)]
mod tests {
    use super::super::arguments::{
        ChildRegion, ContentNodes, ParsedArguments, ParsedSlot, ParsedSlots, SlotRole,
    };
    use super::super::builder::NodeTreeBuilder;
    use super::super::kind::{CallableData, GroupData, NodeKind};
    use super::super::tree::{next_tree_tag, NodeTree, TreeCore, NO_PARENT};
    use super::*;
    use crate::scopes::ScopeStack;
    use crate::source::{Source, SourceSpan, Span};
    use crate::spec::{CallableSpec, StdCallableSpec};
    use crate::state::{ParsingState, StateData, TrivialLang};
    use crate::token::{
        CommandRules, CommentRules, ForbiddenCharsRules, GroupRules, ParagraphRules,
        SpecialsRules, TokenRules, WhitespaceRules,
    };
    use alloc::sync::Arc;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl TrivialLang for PlainLang {}

    fn state() -> Arc<ParsingState<PlainLang>> {
        Arc::new(ParsingState::new(StateData {
            rules: TokenRules {
                whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
                paragraphs: ParagraphRules { enabled: true },
                groups: GroupRules {
                    enabled: true,
                    rules: alloc::vec::Vec::new(),
                    temporary: alloc::vec::Vec::new(),
                    expecting_close: None,
                },
                commands: CommandRules {
                    enabled: true,
                    rules: alloc::vec::Vec::new(),
                },
                comments: CommentRules {
                    enabled: true,
                    rules: alloc::vec::Vec::new(),
                },
                specials: SpecialsRules { enabled: true },
                forbidden_chars: ForbiddenCharsRules { chars: "".into() },
            },
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }))
    }

    /// `"a{bc}d"` hand-built correctly: root List [chars, group [chars], chars].
    fn build_valid() -> NodeTree<PlainLang> {
        let source: Arc<Source> = Arc::new(Source::new("a{bc}d"));
        let st = state();
        let mut builder: NodeTreeBuilder<PlainLang> = NodeTreeBuilder::new();
        let a = builder.add(
            NodeKind::chars(Span::new(0, 1)),
            SourceSpan::new(&source, 0..1),
            Arc::clone(&st),
            alloc::vec![], (), (),
        ).unwrap();
        let bc = builder.add(
            NodeKind::chars(Span::new(2, 4)),
            SourceSpan::new(&source, 2..4),
            Arc::clone(&st),
            alloc::vec![], (), (),
        ).unwrap();
        let group = builder.add(
            NodeKind::group(GroupData::new(0u32, Span::new(1, 2), Span::new(4, 5))),
            SourceSpan::new(&source, 1..5),
            Arc::clone(&st),
            alloc::vec![bc], (), (),
        ).unwrap();
        let d = builder.add(
            NodeKind::chars(Span::new(5, 6)),
            SourceSpan::new(&source, 5..6),
            Arc::clone(&st),
            alloc::vec![], (), (),
        ).unwrap();
        let root = builder.add(
            NodeKind::list(),
            SourceSpan::entire(&source),
            Arc::clone(&st),
            alloc::vec![a, group, d], (), (),
        ).unwrap();
        builder.finish(root).unwrap()
    }

    #[test]
    fn as_str_answers_the_bare_variant_name() {
        // The duplicate name table below is deliberate and exhaustive (no `_` arm):
        // adding or renaming a variant fails compilation here, keeping `as_str` in
        // step.
        fn expected(kind: &TreeViolationKind) -> &'static str {
            match kind {
                TreeViolationKind::Empty => "Empty",
                TreeViolationKind::ChildrenOutOfBounds { .. } => "ChildrenOutOfBounds",
                TreeViolationKind::ChildrenNotAfterParent { .. } => "ChildrenNotAfterParent",
                TreeViolationKind::MultipleParents { .. } => "MultipleParents",
                TreeViolationKind::RootHasParent { .. } => "RootHasParent",
                TreeViolationKind::Unreachable => "Unreachable",
                TreeViolationKind::LeafWithChildren { .. } => "LeafWithChildren",
                TreeViolationKind::UnresolvedRegion { .. } => "UnresolvedRegion",
                TreeViolationKind::RegionNotTiling { .. } => "RegionNotTiling",
                TreeViolationKind::ChildrenNotInRegions { .. } => "ChildrenNotInRegions",
                TreeViolationKind::ContentOutsideRegion { .. } => "ContentOutsideRegion",
                TreeViolationKind::ContentOutsideContentParent { .. } => {
                    "ContentOutsideContentParent"
                }
                TreeViolationKind::ContentParentOutsideSubtree { .. } => {
                    "ContentParentOutsideSubtree"
                }
                TreeViolationKind::ContentParentOutsideRegion { .. } => {
                    "ContentParentOutsideRegion"
                }
                TreeViolationKind::SpannedContentInvalid { .. } => "SpannedContentInvalid",
            }
        }

        let tree = build_valid();
        let id = tree.root().id();
        let kinds = alloc::vec![
            TreeViolationKind::Empty,
            TreeViolationKind::ChildrenOutOfBounds { children: 0..9, node_count: 5 },
            TreeViolationKind::ChildrenNotAfterParent { children: 0..1 },
            TreeViolationKind::MultipleParents { first_parent: id, second_parent: id },
            TreeViolationKind::RootHasParent { parent: id },
            TreeViolationKind::Unreachable,
            TreeViolationKind::LeafWithChildren { kind: "Chars" },
            TreeViolationKind::UnresolvedRegion { region: 0 },
            TreeViolationKind::RegionNotTiling {
                region: 0,
                children: 1..2,
                block: 1..2,
                expected_start: 1,
            },
            TreeViolationKind::ChildrenNotInRegions { unassigned: 1..2 },
            TreeViolationKind::ContentOutsideRegion {
                region: 0,
                content: 1..2,
                region_children: 1..2,
            },
            TreeViolationKind::ContentOutsideContentParent {
                region: 0,
                content: 1..2,
                parent_children: 1..2,
            },
            TreeViolationKind::ContentParentOutsideSubtree { region: 0, content_parent: id },
            TreeViolationKind::ContentParentOutsideRegion {
                region: 0,
                content_parent: id,
                region_children: 1..2,
            },
            TreeViolationKind::SpannedContentInvalid {
                what: "chars content",
                span: Span::new(0, 1),
                content_len: 1,
            },
        ];
        for kind in &kinds {
            assert_eq!(kind.as_str(), expected(kind));
        }
    }

    /// Assemble a tree directly from node data, bypassing the builder's validation —
    /// the only way to construct all-trees-law violations, since `finish()` rejects
    /// them (in-crate hand shape; the placeholder parent table is never read:
    /// `validate_tree` recomputes the relation).
    fn hand_tree(nodes: alloc::vec::Vec<NodeData<PlainLang>>) -> NodeTree<PlainLang> {
        let n = nodes.len();
        NodeTree {
            core: Arc::new(TreeCore {
                nodes,
                parent: alloc::vec![NO_PARENT; n],
                tree_tag: next_tree_tag(),
                single_source: false,
            }),
            annotations: alloc::vec![(); n],
        }
    }

    fn node_data(
        kind: NodeKind<PlainLang>,
        source: &Arc<Source>,
        span: core::ops::Range<usize>,
        children: core::ops::Range<u32>,
    ) -> NodeData<PlainLang> {
        NodeData {
            kind,
            ext: (),
            span: SourceSpan::new(source, span),
            parsing_state: state(),
            children,
        }
    }

    // --- validate_tree: the all-trees law ---------------------------------------------

    #[test]
    fn validate_accepts_a_valid_tree() {
        validate_tree(&build_valid()).unwrap();
        validate_tree(&build_valid().materialize()).unwrap();
    }

    #[test]
    fn validate_reports_multiple_parents() {
        // Root List claims 1..3; node 1 (List) also claims node 2.
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let tree = hand_tree(alloc::vec![
            node_data(NodeKind::list(), &source, 0..2, 1..3),
            node_data(NodeKind::list(), &source, 0..2, 2..3),
            node_data(NodeKind::chars(Span::new(0, 2)), &source, 0..2, 0..0),
        ]);
        let violation = validate_tree(&tree).unwrap_err();
        assert_eq!(violation.node, Some(tree.make_id(2)));
        assert_eq!(
            violation.kind,
            TreeViolationKind::MultipleParents {
                first_parent: tree.make_id(0),
                second_parent: tree.make_id(1),
            }
        );
    }

    #[test]
    fn validate_reports_unreachable_nodes() {
        // Root claims nothing; node 1 is inside no children range.
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let tree = hand_tree(alloc::vec![
            node_data(NodeKind::list(), &source, 0..2, 0..0),
            node_data(NodeKind::chars(Span::new(0, 2)), &source, 0..2, 0..0),
        ]);
        let violation = validate_tree(&tree).unwrap_err();
        assert_eq!(violation.node, Some(tree.make_id(1)));
        assert_eq!(violation.kind, TreeViolationKind::Unreachable);
    }

    #[test]
    fn validate_reports_broken_region_tiling() {
        // A callable whose one slot region starts past the children block's start.
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let mut region = ChildRegion::new(0..1, ContentNodes::InRegion(0..1));
        let callable = CallableData {
            callable_type: 0u32,
            name: "m".into(),
            spec: Arc::new(StdCallableSpec::default()) as Arc<dyn CallableSpec<PlainLang>>,
            arguments: ParsedArguments::empty(),
            slots: ParsedSlots::new(alloc::vec![]),
            invocation_syntax: (),
        };
        let tree = {
            let mut nodes = alloc::vec![
                node_data(NodeKind::list(), &source, 0..2, 1..2),
                node_data(NodeKind::callable(callable), &source, 0..2, 2..3),
                node_data(NodeKind::chars(Span::new(0, 2)), &source, 0..2, 0..0),
            ];
            let tag = next_tree_tag();
            // The region claims 3..3 — the block 2..3 expects it to start at 2.
            region.resolve(3..3, 3..3, 1, tag);
            match &mut nodes[1].kind {
                NodeKind::Callable(data) => {
                    data.slots =
                        ParsedSlots::new(alloc::vec![ParsedSlot::new(
                            region,
                            "s",
                            SlotRole::Content,
                            ()
                        )]);
                }
                _ => unreachable!("node 1 is the callable"),
            }
            let n = nodes.len();
            NodeTree {
                core: Arc::new(TreeCore {
                    nodes,
                    parent: alloc::vec![NO_PARENT; n],
                    tree_tag: tag,
                    single_source: false,
                }),
                annotations: alloc::vec![(); n],
            }
        };
        let violation = validate_tree(&tree).unwrap_err();
        assert_eq!(violation.node, Some(tree.make_id(1)));
        assert_eq!(
            violation.kind,
            TreeViolationKind::RegionNotTiling {
                region: 0,
                children: 3..3,
                block: 2..3,
                expected_start: 2,
            }
        );
    }

    #[test]
    fn validate_reports_non_resident_payloads() {
        // Spanned content 0..99 of a 2-byte source: residency, not positional pinning.
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let tree = hand_tree(alloc::vec![node_data(
            NodeKind::chars(Span::new(0, 99)),
            &source,
            0..2,
            0..0
        )]);
        let violation = validate_tree(&tree).unwrap_err();
        assert_eq!(violation.node, Some(tree.make_id(0)));
        assert_eq!(
            violation.kind,
            TreeViolationKind::SpannedContentInvalid {
                what: "chars content",
                span: Span::new(0, 99),
                content_len: 2,
            }
        );
    }

    #[test]
    fn validate_does_no_byte_accounting() {
        // A sibling gap (bytes 0..1 unrepresented) and off-span content: parse-law
        // violations, but the all-trees law passes.
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let st = state();
        let mut builder: NodeTreeBuilder<PlainLang> = NodeTreeBuilder::new();
        let b = builder.add(
            NodeKind::chars(Span::new(1, 2)),
            SourceSpan::new(&source, 1..2),
            Arc::clone(&st),
            alloc::vec![], (), (),
        ).unwrap();
        let root = builder.add(
            NodeKind::list(),
            SourceSpan::entire(&source),
            Arc::clone(&st),
            alloc::vec![b], (), (),
        ).unwrap();
        validate_tree(&builder.finish(root).unwrap()).unwrap();
    }

    // --- check_tree_invariants: the parse-law oracle ----------------------------------

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
            alloc::vec![], (), (),
        ).unwrap();
        let root = builder.add(
            NodeKind::list(),
            SourceSpan::entire(&source),
            Arc::clone(&st),
            alloc::vec![b], (), (),
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
            alloc::vec![], (), (),
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
            alloc::vec![], (), (),
        ).unwrap();
        check_tree_invariants(&builder.finish(root).unwrap());
    }

    // --- per-source byte accounting via the slot roles ([§dd-dr:slot-roles]) ----------

    /// Build `\input`-shaped trees: a callable in `main` whose one slot (role
    /// `role`) holds children with the given spans in the given sources. The
    /// callable spans the whole 9-byte main source; the root list wraps it.
    fn build_with_slot(
        role: SlotRole,
        children_spans: alloc::vec::Vec<(Arc<Source>, core::ops::Range<usize>)>,
    ) -> NodeTree<PlainLang> {
        let main: Arc<Source> = Arc::new(Source::new(r"\input{f}"));
        let st = state();
        let mut builder: NodeTreeBuilder<PlainLang> = NodeTreeBuilder::new();
        let mut children = alloc::vec::Vec::new();
        for (source, range) in &children_spans {
            children.push(
                builder.add(
                    NodeKind::chars(Span::new(range.start, range.end)),
                    SourceSpan::new(source, range.clone()),
                    Arc::clone(&st),
                    alloc::vec![], (), (),
                ).unwrap(),
            );
        }
        let n = children.len() as u32;
        let callable = CallableData {
            callable_type: 0u32,
            name: "input".into(),
            spec: Arc::new(StdCallableSpec::default()) as Arc<dyn CallableSpec<PlainLang>>,
            arguments: ParsedArguments::empty(),
            slots: ParsedSlots::new(alloc::vec![ParsedSlot::new(
                ChildRegion::new(0..n, ContentNodes::InRegion(0..n)),
                "attached",
                role,
                (),
            )]),
            invocation_syntax: (),
        };
        let callable_id = builder.add(
            NodeKind::callable(callable),
            SourceSpan::entire(&main),
            Arc::clone(&st),
            children, (), (),
        ).unwrap();
        let root = builder.add(
            NodeKind::list(),
            SourceSpan::entire(&main),
            Arc::clone(&st),
            alloc::vec![callable_id], (), (),
        ).unwrap();
        builder.finish(root).unwrap()
    }

    #[test]
    fn attached_slot_children_are_excluded_from_the_parents_accounting() {
        // The `\input` shape: the slot's children live in the attached source —
        // excluded from the includer's children-in-source/contiguity checks,
        // with their own per-source accounting (contiguous in their source).
        let attached: Arc<Source> = Arc::new(Source::new("ab"));
        let tree = build_with_slot(
            SlotRole::Attached,
            alloc::vec![(Arc::clone(&attached), 0..1), (attached, 1..2)],
        );
        check_tree_invariants(&tree);
    }

    #[test]
    #[should_panic(expected = "attached region 2..4 not span-contiguous")]
    fn attached_region_children_must_be_contiguous_in_their_own_source() {
        let attached: Arc<Source> = Arc::new(Source::new("abc"));
        let tree = build_with_slot(
            SlotRole::Attached,
            alloc::vec![(Arc::clone(&attached), 0..1), (attached, 2..3)],
        );
        check_tree_invariants(&tree);
    }

    #[test]
    #[should_panic(expected = "attached region 2..4 spans several sources")]
    fn attached_region_children_must_share_one_source() {
        let first: Arc<Source> = Arc::new(Source::new("ab"));
        let second: Arc<Source> = Arc::new(Source::new("ab"));
        let tree = build_with_slot(
            SlotRole::Attached,
            alloc::vec![(first, 0..1), (second, 1..2)],
        );
        check_tree_invariants(&tree);
    }

    #[test]
    #[should_panic(expected = "lives in a different source")]
    fn content_slot_children_stay_under_the_parents_accounting() {
        // The same foreign-source children under a `Content` slot: the exclusion
        // is *declared* by the role, never inferred from a source change.
        let attached: Arc<Source> = Arc::new(Source::new("ab"));
        let tree = build_with_slot(
            SlotRole::Content,
            alloc::vec![(Arc::clone(&attached), 0..1), (attached, 1..2)],
        );
        check_tree_invariants(&tree);
    }

    #[test]
    fn hidden_slot_children_carry_no_byte_accounting_at_all() {
        // `Hidden` = no byte accounting ([§dd-dr:slot-roles]): a different
        // source AND a gap inside the region — no assertion fires.
        let foreign: Arc<Source> = Arc::new(Source::new("abc"));
        let tree = build_with_slot(
            SlotRole::Hidden,
            alloc::vec![(Arc::clone(&foreign), 0..1), (foreign, 2..3)],
        );
        check_tree_invariants(&tree);
    }

    #[test]
    #[should_panic(expected = "node 0: chars content span")]
    fn wrapper_panic_carries_the_violation_detail() {
        // The panic message is the violation's Display rendering — node + detail.
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let tree = hand_tree(alloc::vec![node_data(
            NodeKind::chars(Span::new(0, 99)),
            &source,
            0..2,
            0..0
        )]);
        check_tree_invariants(&tree);
    }
}
