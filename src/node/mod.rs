//! The node tree: flat, frozen, index-based AST storage (ARCHITECTURE.md §nodes,
//! Decision 3).
//!
//! - [`NodeTree`] stores all nodes of a parse in one `Vec`; a node's children occupy a
//!   contiguous index block (`Range<u32>`). Trees are immutable — they come out of a
//!   [`NodeTreeBuilder`] (driven by `ParserSession`, Phase 6) and are only read
//!   afterwards, through [`NodeRef`] proxies. Transformations build new trees;
//!   Arc-shared sources, specs, and states make that cheap.
//! - [`NodeKind`] is the **closed structural core**: `Chars` / `Group` / `Callable` /
//!   `Comment` / `List` — no `Custom` variant, no invocation-form variants ("environment"
//!   is a preset concept). Custom data rides in the two-tier ext system
//!   ([`NodeExtTypes`] bundle, `Lang::NodeExts`), orthogonal
//!   to structural identity.
//! - [`CallableData`] records the **invocation facts** (form, spelling, layout,
//!   post-space); shared behavior lives in the spec, context in the recorded parsing
//!   state (the division-of-labor rule).
//! - **One node per region** (Phase 5 design session, DESIGN_RATIONALE.md §3.5): a
//!   callable's children are one node per *present* argument followed by one `List` node
//!   per slot; [`ArgsLayout`]/[`SlotsLayout`] map spec positions to child offsets and
//!   record per-instance syntax choices.
//! - Node textual payloads are [`TextContent`](crate::source::TextContent) (span-backed
//!   or owned); [`NodeTree::materialize`] produces an all-owned copy. Names are always
//!   owned (identity vs. content ownership rule).

mod builder;
mod kind;
mod layout;
mod node_ref;
mod tree;

pub use builder::{BuildId, NodeTreeBuilder};
pub use kind::{CallableData, NodeKind};
pub use layout::{ArgLayout, ArgsLayout, SlotLayout, SlotsLayout};
pub use node_ref::NodeRef;
pub use tree::{NodeData, NodeId, NodeTree};

use crate::state::{Lang, NodeExtTypes};

/// The uniform (tier-1) node ext type of a language.
pub type NodeExt<L> = <<L as Lang>::NodeExts as NodeExtTypes>::NodeExt;
/// The `Chars` (tier-2) node ext type of a language.
pub type CharsNodeExt<L> = <<L as Lang>::NodeExts as NodeExtTypes>::CharsNodeExt;
/// The `Group` (tier-2) node ext type of a language.
pub type GroupNodeExt<L> = <<L as Lang>::NodeExts as NodeExtTypes>::GroupNodeExt;
/// The `Callable` (tier-2) node ext type of a language.
pub type CallableNodeExt<L> = <<L as Lang>::NodeExts as NodeExtTypes>::CallableNodeExt;
/// The `Comment` (tier-2) node ext type of a language.
pub type CommentNodeExt<L> = <<L as Lang>::NodeExts as NodeExtTypes>::CommentNodeExt;
/// The `List` (tier-2) node ext type of a language.
pub type ListNodeExt<L> = <<L as Lang>::NodeExts as NodeExtTypes>::ListNodeExt;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::LibraryStack;
    use crate::source::{Source, SourceSpan, Span, TextContent};
    use crate::spec::{CallableSpec, CallableTypeId, StdCallableSpec};
    use crate::state::{ParsingState, SimpleLang, StateData};
    use crate::token::{GroupType, GroupTypeId, TokenRules};
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::ops::Range;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl SimpleLang for PlainLang {}

    const GT_BRACE: GroupTypeId = GroupTypeId::new(0);
    const CT_MACRO: CallableTypeId = CallableTypeId::new(0);

    fn min_rules() -> TokenRules {
        TokenRules {
            whitespace: None,
            double_newline_paragraphs: false,
            group_types: vec![GroupType { id: GT_BRACE, open: "{".into(), close: "}".into() }],
            commands: Vec::new(),
            comments: Vec::new(),
            forbidden_chars: String::new(),
            expecting_group_close: None,
        }
    }

    fn state<L: Lang<StateExt = ()>>() -> Arc<ParsingState<L>> {
        Arc::new(ParsingState::new(StateData {
            rules: min_rules(),
            libraries: LibraryStack::new(),
            ext: (),
        }))
    }

    fn spanned(source: &Arc<Source>, range: Range<usize>) -> SourceSpan {
        SourceSpan::new(source, range)
    }

    /// Builds the running example: `x\frac{a}{b} % note` as
    /// List [ Chars"x", Callable\frac(Group(Chars"a"), Group(Chars"b")), Comment" note" ].
    fn example_tree() -> NodeTree<PlainLang> {
        let source: Arc<Source> = Arc::new(Source::new(r"x\frac{a}{b} % note"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();

        let x = b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![]);

        let a_chars =
            b.add(NodeKind::chars(Span::new(7, 8)), spanned(&source, 7..8), st.clone(), vec![]);
        let a_group =
            b.add(NodeKind::group(GT_BRACE), spanned(&source, 6..9), st.clone(), vec![a_chars]);
        let b_chars =
            b.add(NodeKind::chars(Span::new(10, 11)), spanned(&source, 10..11), st.clone(), vec![]);
        let b_group =
            b.add(NodeKind::group(GT_BRACE), spanned(&source, 9..12), st.clone(), vec![b_chars]);

        let spec: Arc<dyn CallableSpec<PlainLang>> = Arc::new(StdCallableSpec::default());
        let frac = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "frac".into(),
                spec,
                args: vec![ArgLayout::Present { child: 0 }, ArgLayout::Present { child: 1 }]
                    .into(),
                slots: SlotsLayout::empty(),
                post_space: TextContent::Spanned(Span::new(12, 13)),
                ext: (),
            }),
            spanned(&source, 1..13), // includes post_space (§nodes span convention)
            st.clone(),
            vec![a_group, b_group],
        );

        let comment = b.add(
            NodeKind::comment(Span::new(14, 19)),
            spanned(&source, 13..19),
            st.clone(),
            vec![],
        );

        let root =
            b.add(NodeKind::list(), SourceSpan::entire(&source), st.clone(), vec![x, frac, comment]);
        b.finish(root)
    }

    #[test]
    fn tree_structure_and_accessors() {
        let tree = example_tree();
        let root = tree.root();
        assert!(root.is_list());
        assert_eq!(root.child_count(), 3);
        assert_eq!(tree.node_count(), 8);

        let x = root.child(0).unwrap();
        assert_eq!(x.chars(), Some("x"));
        assert_eq!(x.span_content(), "x");
        assert!(x.comment().is_none());

        let frac = root.child(1).unwrap();
        assert!(frac.is_callable());
        assert_eq!(frac.name(), Some("frac"));
        assert_eq!(frac.callable_type(), Some(CT_MACRO));
        assert_eq!(frac.post_space(), Some(" "));
        assert_eq!(frac.span_content(), r"\frac{a}{b} ");
        assert!(frac.spec().is_some());

        let comment = root.child(2).unwrap();
        assert_eq!(comment.comment(), Some(" note"));
        assert!(comment.chars().is_none());

        assert!(root.child(3).is_none());
        // children() iterates in order:
        let kinds: Vec<bool> = root.children().map(|c| c.is_callable()).collect();
        assert_eq!(kinds, [false, true, false]);
    }

    #[test]
    fn argument_access() {
        let tree = example_tree();
        let frac = tree.root().child(1).unwrap();

        let arg0 = frac.argument(0).unwrap();
        assert!(arg0.is_group());
        assert_eq!(arg0.group_type(), Some(GT_BRACE));
        assert_eq!(arg0.span_content(), "{a}");
        assert_eq!(arg0.child(0).unwrap().chars(), Some("a"));

        let arg1 = frac.argument(1).unwrap();
        assert_eq!(arg1.span_content(), "{b}");

        assert!(frac.argument(2).is_none());
        assert_eq!(frac.args_layout().unwrap().len(), 2);

        let args: Vec<_> = frac.arguments().collect();
        assert_eq!(args.len(), 2);
        assert!(args.iter().all(|(layout, node)| layout.is_present() && node.is_some()));

        // Non-callables answer None everywhere:
        let x = tree.root().child(0).unwrap();
        assert!(x.argument(0).is_none());
        assert!(x.name().is_none());
        assert_eq!(x.arguments().count(), 0);
    }

    #[test]
    fn absent_and_marker_arguments() {
        // \section*{title}-shaped: star marker present, optional arg absent, one group arg.
        let source: Arc<Source> = Arc::new(Source::new(r"\section*{t}"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();

        let t = b.add(NodeKind::chars(Span::new(10, 11)), spanned(&source, 10..11), st.clone(), vec![]);
        let title = b.add(NodeKind::group(GT_BRACE), spanned(&source, 9..12), st.clone(), vec![t]);

        let spec: Arc<dyn CallableSpec<PlainLang>> = Arc::new(StdCallableSpec::default());
        let section = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "section".into(),
                spec,
                args: vec![
                    ArgLayout::Marker { text: TextContent::Spanned(Span::new(8, 9)) },
                    ArgLayout::Absent,
                    ArgLayout::Present { child: 0 },
                ]
                .into(),
                slots: SlotsLayout::empty(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            SourceSpan::entire(&source),
            st.clone(),
            vec![title],
        );
        let tree = b.finish(section);

        let node = tree.root();
        // Marker and absent arguments have no node; the group argument maps to child 0.
        assert!(node.argument(0).is_none());
        assert!(node.argument(1).is_none());
        assert_eq!(node.argument(2).unwrap().span_content(), "{t}");

        let layout = node.args_layout().unwrap();
        assert!(layout.get(0).unwrap().is_present());
        assert!(!layout.get(1).unwrap().is_present());
        assert_eq!(layout.get(2).unwrap().child(), Some(0));
        match layout.get(0).unwrap() {
            ArgLayout::Marker { text } => {
                assert_eq!(text.resolve(node.span().source().content()), "*")
            }
            other => panic!("expected marker, got {:?}", other),
        }
    }

    #[test]
    fn slots_and_body() {
        // An environment-shaped callable: one body slot (a List child after no args).
        let source: Arc<Source> = Arc::new(Source::new(r"\begin{it}hi\end{it}"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();

        let hi = b.add(NodeKind::chars(Span::new(10, 12)), spanned(&source, 10..12), st.clone(), vec![]);
        let body = b.add(NodeKind::list(), spanned(&source, 10..12), st.clone(), vec![hi]);

        let spec: Arc<dyn CallableSpec<PlainLang>> = Arc::new(StdCallableSpec::default());
        let env = b.add(
            NodeKind::callable(CallableData {
                callable_type: CallableTypeId::new(1),
                name: "it".into(),
                spec,
                args: ArgsLayout::empty(),
                slots: vec![SlotLayout { child: 0 }].into(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            SourceSpan::entire(&source),
            st.clone(),
            vec![body],
        );
        let tree = b.finish(env);

        let node = tree.root();
        let body = node.body().unwrap();
        assert!(body.is_list());
        assert_eq!(body.child_count(), 1);
        assert_eq!(body.child(0).unwrap().chars(), Some("hi"));
        assert_eq!(node.slot(0).unwrap().id(), body.id());
        assert!(node.slot(1).is_none());
        assert_eq!(node.slots_layout().unwrap().len(), 1);
    }

    #[test]
    fn sibling_ranges_are_contiguous_and_flat() {
        let tree = example_tree();
        // Every node's children ids are a contiguous ascending run, and each non-root
        // node is the child of exactly one node.
        let mut seen = vec![0usize; tree.node_count()];
        for node in tree.iter() {
            let ids: Vec<usize> = node.children().map(|c| c.id().index()).collect();
            for pair in ids.windows(2) {
                assert_eq!(pair[1], pair[0] + 1);
            }
            for id in ids {
                seen[id] += 1;
            }
        }
        assert_eq!(tree.root().id().index(), 0);
        assert_eq!(seen[0], 0); // root: no parent
        assert!(seen[1..].iter().all(|&n| n == 1));
    }

    #[test]
    fn unreachable_staged_nodes_are_dropped() {
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let _abandoned =
            b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![]);
        let kept = b.add(NodeKind::chars(Span::new(1, 2)), spanned(&source, 1..2), st.clone(), vec![]);
        let root = b.add(NodeKind::list(), SourceSpan::entire(&source), st, vec![kept]);
        let tree = b.finish(root);
        assert_eq!(tree.node_count(), 2);
        assert_eq!(tree.root().child(0).unwrap().chars(), Some("b"));
    }

    #[test]
    fn materialize_owns_all_text_and_preserves_it() {
        let tree = example_tree();
        let owned = tree.materialize();

        // Logical text identical…
        let root = owned.root();
        assert_eq!(root.child(0).unwrap().chars(), Some("x"));
        assert_eq!(root.child(1).unwrap().post_space(), Some(" "));
        assert_eq!(root.child(2).unwrap().comment(), Some(" note"));
        // …but stored owned:
        for node in owned.iter() {
            match node.kind() {
                NodeKind::Chars { content, .. } | NodeKind::Comment { content, .. } => {
                    assert!(content.is_owned())
                }
                NodeKind::Callable(data) => assert!(data.post_space.is_owned()),
                _ => {}
            }
        }
        // The original tree is untouched (still span-backed):
        match tree.root().child(0).unwrap().kind() {
            NodeKind::Chars { content, .. } => assert!(!content.is_owned()),
            _ => unreachable!(),
        }
        // Spans (provenance) survive materialization:
        assert_eq!(owned.root().child(1).unwrap().span_content(), r"\frac{a}{b} ");
    }

    // --- ext plumbing ---------------------------------------------------------------

    #[derive(Debug, Clone, Default, PartialEq)]
    struct CharsMeta {
        weight: u32,
    }

    struct ExtBundle;
    impl crate::state::NodeExtTypes for ExtBundle {
        type NodeExt = u16; // e.g. a bindings-handle index
        type CharsNodeExt = CharsMeta;
        type GroupNodeExt = ();
        type CallableNodeExt = ();
        type CommentNodeExt = ();
        type ListNodeExt = ();
    }

    #[derive(Debug, Clone, Copy)]
    struct ExtLang;
    impl Lang for ExtLang {
        type StateExt = ();
        type Event = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ExtBundle;
    }

    #[test]
    fn ext_types_are_stored_and_read_back() {
        let source: Arc<Source> = Arc::new(Source::new("y"));
        let st = state::<ExtLang>();
        let mut b = NodeTreeBuilder::new();
        let y = b.add_with_ext(
            NodeKind::Chars {
                content: TextContent::Spanned(Span::new(0, 1)),
                ext: CharsMeta { weight: 7 },
            },
            spanned(&source, 0..1),
            st.clone(),
            vec![],
            42u16,
        );
        let root = b.add(NodeKind::list(), SourceSpan::entire(&source), st, vec![y]);
        let tree = b.finish(root);

        let y = tree.root().child(0).unwrap();
        assert_eq!(*y.ext(), 42);
        match y.kind() {
            NodeKind::Chars { ext, .. } => assert_eq!(*ext, CharsMeta { weight: 7 }),
            _ => unreachable!(),
        }
        // Default tier-1 ext via plain add():
        assert_eq!(*tree.root().ext(), 0);
    }

    // --- Debug does not demand anything of the lang ZST itself -----------------------

    struct NoDeriveLang; // deliberately neither Clone nor Debug
    impl SimpleLang for NoDeriveLang {}

    #[test]
    fn debug_and_clone_without_lang_bounds() {
        let source: Arc<Source> = Arc::new(Source::new("z"));
        let st = state::<NoDeriveLang>();
        let mut b = NodeTreeBuilder::new();
        let z = b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![]);
        let root = b.add(NodeKind::list(), SourceSpan::entire(&source), st, vec![z]);
        let tree = b.finish(root);

        let cloned = tree.clone();
        let dump = alloc::format!("{:?}", cloned);
        assert!(dump.contains("Chars"));
        let node_dump = alloc::format!("{:?}", tree.root().child(0).unwrap());
        assert!(node_dump.contains("NodeId(1)"));
    }

    #[test]
    #[should_panic(expected = "already has a parent")]
    fn a_node_cannot_have_two_parents() {
        let source: Arc<Source> = Arc::new(Source::new("w"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let w = b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![]);
        let _l1 = b.add(NodeKind::list(), SourceSpan::entire(&source), st.clone(), vec![w]);
        let _l2 = b.add(NodeKind::list(), SourceSpan::entire(&source), st, vec![w]);
    }
}
