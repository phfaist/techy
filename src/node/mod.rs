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
//! - [`GroupData`] records a group's delimiters *on the node* (pylatexenc's
//!   `delimiters`), alongside its optional typed identity.
//! - [`CallableData`] records the **invocation facts** (form, spelling, parsed
//!   arguments/slots, post-space); shared behavior lives in the spec, context in the
//!   recorded parsing state (the division-of-labor rule).
//! - **One node per region** (Phase 5 design session, DESIGN_RATIONALE.md §3.5): a
//!   callable's children are one node per *provided* argument followed by one `List`
//!   node per slot; [`ParsedArguments`]/[`ParsedSlots`] (pylatexenc's `ParsedArguments`
//!   pattern, July 2026) map each region to its child offset, record which
//!   [`ArgumentSpec`](crate::spec::ArgumentSpec) it was parsed against, and hold
//!   per-instance syntax records.
//! - Node textual payloads are [`TextContent`](crate::source::TextContent) (span-backed
//!   or owned); [`NodeTree::materialize`] produces an all-owned copy. Names are always
//!   owned (identity vs. content ownership rule).

mod arguments;
mod builder;
mod kind;
mod node_ref;
mod tree;

pub use arguments::{ParsedArgument, ParsedArguments, ParsedSlot, ParsedSlots};
pub use builder::{BuildId, NodeTreeBuilder};
pub use kind::{CallableData, GroupData, NodeKind};
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
/// The parsed-argument ext type of a language (attached to [`ParsedArgument`] records).
pub type ArgumentExt<L> = <<L as Lang>::NodeExts as NodeExtTypes>::ArgumentExt;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::LibraryStack;
    use crate::source::{Source, SourceSpan, Span, TextContent};
    use crate::spec::{ArgumentSpec, CallableSpec, SlotSpec, StdCallableSpec};
    use crate::state::{Lang, ParsingState, SimpleLang, StateData};
    use crate::token::{GroupType, TokenRules};
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::ops::Range;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl SimpleLang for PlainLang {} // GroupTypeId / CallableTypeId = u32

    const GT_BRACE: u32 = 0;
    const CT_MACRO: u32 = 0;
    const CT_ENVIRONMENT: u32 = 1;

    fn min_rules<L: Lang<GroupTypeId = u32>>() -> TokenRules<L> {
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

    fn state<L: Lang<StateExt = (), GroupTypeId = u32>>() -> Arc<ParsingState<L>> {
        Arc::new(ParsingState::new(StateData {
            rules: min_rules(),
            libraries: LibraryStack::new(),
            ext: (),
        }))
    }

    fn spanned(source: &Arc<Source>, range: Range<usize>) -> SourceSpan {
        SourceSpan::new(source, range)
    }

    fn brace_group<L: Lang<GroupTypeId = u32>>(open: Range<usize>, close: Range<usize>) -> GroupData<L> {
        GroupData::new(
            GT_BRACE,
            TextContent::Spanned(Span::new(open.start, open.end)),
            TextContent::Spanned(Span::new(close.start, close.end)),
        )
    }

    fn brace_arg_spec<L: Lang<GroupTypeId = u32>>() -> Arc<ArgumentSpec<L>> {
        Arc::new(ArgumentSpec::group(GT_BRACE))
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
        let a_group = b.add(
            NodeKind::group(brace_group(6..7, 8..9)),
            spanned(&source, 6..9),
            st.clone(),
            vec![a_chars],
        );
        let b_chars =
            b.add(NodeKind::chars(Span::new(10, 11)), spanned(&source, 10..11), st.clone(), vec![]);
        let b_group = b.add(
            NodeKind::group(brace_group(9..10, 11..12)),
            spanned(&source, 9..12),
            st.clone(),
            vec![b_chars],
        );

        let arg_specs = [brace_arg_spec(), brace_arg_spec()];
        let spec: Arc<dyn CallableSpec<PlainLang>> =
            Arc::new(StdCallableSpec::new(arg_specs.to_vec(), vec![]));
        let frac = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "frac".into(),
                spec,
                arguments: vec![
                    ParsedArgument::provided(arg_specs[0].clone(), 0),
                    ParsedArgument::provided(arg_specs[1].clone(), 1),
                ]
                .into(),
                slots: ParsedSlots::empty(),
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
        assert_eq!(arg0.group_delimiters(), Some(("{", "}")));
        assert_eq!(arg0.span_content(), "{a}");
        assert_eq!(arg0.child(0).unwrap().chars(), Some("a"));

        let arg1 = frac.argument(1).unwrap();
        assert_eq!(arg1.span_content(), "{b}");

        assert!(frac.argument(2).is_none());
        let args = frac.arguments().unwrap();
        assert_eq!(args.len(), 2);
        assert!(args.iter().all(|arg| arg.is_provided()));
        // Every parsed argument knows the spec it was parsed against:
        assert!(args.iter().all(|arg| Arc::strong_count(&arg.spec) >= 2));

        // Non-callables answer None everywhere:
        let x = tree.root().child(0).unwrap();
        assert!(x.argument(0).is_none());
        assert!(x.name().is_none());
        assert!(x.arguments().is_none());
    }

    #[test]
    fn absent_marker_and_named_arguments() {
        // \section*{t}-shaped: star marker provided (a Chars node — pylatexenc behavior),
        // optional arg absent, one group arg. All argument specs are named.
        let source: Arc<Source> = Arc::new(Source::new(r"\section*{t}"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();

        let star =
            b.add(NodeKind::chars(Span::new(8, 9)), spanned(&source, 8..9), st.clone(), vec![]);
        let t = b.add(NodeKind::chars(Span::new(10, 11)), spanned(&source, 10..11), st.clone(), vec![]);
        let title = b.add(
            NodeKind::group(brace_group(9..10, 11..12)),
            spanned(&source, 9..12),
            st.clone(),
            vec![t],
        );

        let star_spec: Arc<ArgumentSpec<PlainLang>> =
            Arc::new(ArgumentSpec::marker("*").named("star"));
        let placement_spec: Arc<ArgumentSpec<PlainLang>> =
            Arc::new(ArgumentSpec::optional_group(GT_BRACE).named("placement"));
        let title_spec: Arc<ArgumentSpec<PlainLang>> =
            Arc::new(ArgumentSpec::group(GT_BRACE).named("title"));
        let spec: Arc<dyn CallableSpec<PlainLang>> = Arc::new(StdCallableSpec::new(
            vec![star_spec.clone(), placement_spec.clone(), title_spec.clone()],
            vec![],
        ));
        let section = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "section".into(),
                spec,
                arguments: vec![
                    ParsedArgument::provided(star_spec, 0),
                    ParsedArgument::absent(placement_spec),
                    ParsedArgument::provided(title_spec, 1),
                ]
                .into(),
                slots: ParsedSlots::empty(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            SourceSpan::entire(&source),
            st.clone(),
            vec![star, title],
        );
        let tree = b.finish(section);

        let node = tree.root();
        // The provided marker is an ordinary Chars child node; the absent optional has
        // an entry but no node.
        assert_eq!(node.argument(0).unwrap().chars(), Some("*"));
        assert!(node.argument(1).is_none());
        assert_eq!(node.argument(2).unwrap().span_content(), "{t}");

        let args = node.arguments().unwrap();
        assert!(args.get(0).unwrap().is_provided());
        assert!(!args.get(1).unwrap().is_provided());
        assert_eq!(args.get(2).unwrap().child, Some(1));

        // By-name access — absent arguments keep their spec, so "absent" and "no such
        // argument" stay distinguishable:
        assert_eq!(node.argument_named("title").unwrap().span_content(), "{t}");
        assert!(node.argument_named("placement").is_none());
        assert!(!args.get_named("placement").unwrap().is_provided());
        assert!(args.get_named("nonsense").is_none());
    }

    #[test]
    fn slots_and_body() {
        // An environment-shaped callable: one body slot (a List child after no args).
        let source: Arc<Source> = Arc::new(Source::new(r"\begin{it}hi\end{it}"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();

        let hi = b.add(NodeKind::chars(Span::new(10, 12)), spanned(&source, 10..12), st.clone(), vec![]);
        let body = b.add(NodeKind::list(), spanned(&source, 10..12), st.clone(), vec![hi]);

        let slot_spec: Arc<SlotSpec<PlainLang>> = Arc::new(SlotSpec::new().named("body"));
        let spec: Arc<dyn CallableSpec<PlainLang>> =
            Arc::new(StdCallableSpec::new(vec![], vec![slot_spec.clone()]));
        let env = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_ENVIRONMENT,
                name: "it".into(),
                spec,
                arguments: ParsedArguments::empty(),
                slots: vec![ParsedSlot::new(slot_spec, 0)].into(),
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
        let slots = node.slots().unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots.get_named("body").unwrap().child, 0);
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
        assert_eq!(root.child(1).unwrap().argument(0).unwrap().group_delimiters(), Some(("{", "}")));
        // …but stored owned:
        for node in owned.iter() {
            match node.kind() {
                NodeKind::Chars { content, .. } | NodeKind::Comment { content, .. } => {
                    assert!(content.is_owned())
                }
                NodeKind::Group(data) => {
                    assert!(data.open.is_owned());
                    assert!(data.close.is_owned());
                }
                NodeKind::Callable(data) => {
                    assert!(data.post_space.is_owned());
                    assert!(data.arguments.iter().all(|arg| arg.pre_space.is_owned()));
                }
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

    #[test]
    fn synthesized_groups_may_have_no_group_type() {
        // Internal synthetic groups (not produced by tokenization) carry delimiters but
        // no language group type.
        let source: Arc<Source> = Arc::new(Source::new("y"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let y = b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![]);
        let g = b.add(
            NodeKind::group(GroupData::untyped(TextContent::from("{"), TextContent::from("}"))),
            spanned(&source, 0..1),
            st.clone(),
            vec![y],
        );
        let tree = b.finish(g);

        let group = tree.root();
        assert!(group.is_group());
        assert_eq!(group.group_type(), None);
        assert_eq!(group.group_delimiters(), Some(("{", "}")));
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
        type ArgumentExt = ();
    }

    #[derive(Debug, Clone, Copy)]
    struct ExtLang;
    impl Lang for ExtLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
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
