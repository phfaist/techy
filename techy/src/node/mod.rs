//! The node tree: flat, frozen, index-based AST storage.
//!
//! - [`NodeTree`] stores all nodes of a parse in one `Vec`; a node's children occupy a
//!   contiguous index block (`Range<u32>`). Trees are immutable — they come out of a
//!   [`NodeTreeBuilder`] (driven by `ParserSession`) and are only read
//!   afterwards, through [`NodeRef`] proxies. Transformations build new trees;
//!   Arc-shared sources, specs, and states make that cheap.
//! - [`NodeKind`] is the **closed structural core**: `Chars` / `Group` / `Callable` /
//!   `Comment` / `List` — no `Custom` variant, no invocation-form variants ("environment"
//!   is a preset concept). Custom data rides in the two-tier ext system
//!   ([`NodeExtTypes`] bundle, `Lang::NodeExts`), orthogonal
//!   to structural identity.
//! - [`GroupData`] records a group's delimiters *on the node* (pylatexenc's
//!   `delimiters`), alongside its optional typed class (`Lang::GroupTypeId`).
//! - [`CallableData`] records the **invocation facts** (form, spelling, parsed
//!   arguments/slots, post-space); shared behavior lives in the spec, context in the
//!   recorded parsing state (the division-of-labor rule).
//! - **One child region per argument/slot**: a callable's children are the concatenation of one
//!   contiguous region per *provided* argument, then one per slot — each region holding
//!   noise (comment nodes, whitespace-only `Chars` nodes) alongside the syntax-bearing
//!   nodes. [`ParsedArguments`]/[`ParsedSlots`] (pylatexenc's `ParsedArguments` pattern)
//!   record each region and its parser-designated content nodes ([`ChildRegion`] —
//!   two-phase: staged by parsers, resolved to global node-index ranges by the
//!   builder's `finish()`), and which [`ArgumentSpec`](crate::spec::ArgumentSpec) each
//!   was parsed against.
//! - Node textual payloads are [`TextContent`](crate::source::TextContent) (span-backed
//!   or owned); [`NodeTree::materialize`] produces an all-owned copy. Names are always
//!   owned (identity vs. content ownership rule).

mod arguments;
mod builder;
mod copy;
mod invariants;
mod kind;
mod node_ref;
mod slice;
mod tree;

pub use arguments::{
    ChildRegion, ContentNodes, ParsedArgument, ParsedArguments, ParsedSlot, ParsedSlots,
};
pub use builder::{BuildId, NodeBuildError, NodeTreeBuilder, StagedNodeView, StagedNodes};
pub use invariants::check_tree_invariants;
pub use kind::{CallableData, GroupData, NodeKind};
pub use node_ref::{Descendants, NodeRef};
pub use slice::{NodeSlice, NodeSliceIter};
pub use tree::{NodeId, NodeTree, TreeTag};

// `NodeData` is deliberately NOT re-exported ([§dd-dr:public-visibility-sweep] Theme C):
// it is crate-internal — zero public signatures use it; `NodeRef` is the read API.
// Crate-internal subtree copying, shared with `crate::extract`'s builder helpers.
pub(crate) use copy::copy_subtree_into;

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
/// The parsed-slot ext type of a language (attached to [`ParsedSlot`] records).
pub type SlotExt<L> = <<L as Lang>::NodeExts as NodeExtTypes>::SlotExt;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scopes::ScopeStack;
    use crate::source::{Source, SourceSpan, Span, TextContent};
    use crate::spec::{ArgumentParser, ArgumentSpec, CallableSpec, StdCallableSpec};
    use crate::state::{Lang, ParsingState, TrivialLang, StateData};
    use crate::token::{GroupRule, TokenRules, WhitespaceRules};
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::ops::Range;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl TrivialLang for PlainLang {} // GroupTypeId / CallableTypeId = u32

    const GT_BRACE: u32 = 0;
    const CT_MACRO: u32 = 0;
    const CT_ENVIRONMENT: u32 = 1;

    fn min_rules<L: Lang<GroupTypeId = u32>>() -> TokenRules<L> {
        TokenRules {
            enable_whitespace: false,
            whitespace: WhitespaceRules::default(),
            enable_multi_newline_paragraphs: false,
            enable_groups: true,
            groups: vec![Arc::new(GroupRule {
                group_type: GT_BRACE,
                open: "{".into(),
                close: "}".into(),
            })],
            temporary_groups: Vec::new(),
            enable_commands: true,
            commands: Vec::new(),
            enable_comments: true,
            comments: Vec::new(),
            enable_specials: true,
            forbidden_chars: "".into(),
            expecting_group_close: None,
        }
    }

    fn state<L: Lang<StateExt = (), GroupTypeId = u32>>() -> Arc<ParsingState<L>> {
        Arc::new(ParsingState::new(StateData {
            rules: min_rules(),
            scopes: ScopeStack::new(),
            mode: Default::default(),
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

    /// Stand-in for the standard argument parsers — never invoked; the tests here
    /// hand-build their trees.
    #[derive(Debug)]
    struct StubParser;
    impl<L: Lang> ArgumentParser<L> for StubParser {
        fn parse_argument(
            &self,
            _cx: &mut crate::constructs::ParseContext<'_, '_, L>,
            _spec: &ArgumentSpec<L>,
        ) -> crate::constructs::ConstructParserResult<
            L,
            Option<crate::spec::ParsedArgumentNodes>,
        > {
            Ok(None)
        }
    }

    fn brace_arg_spec<L: Lang<GroupTypeId = u32>>() -> Arc<ArgumentSpec<L>> {
        Arc::new(ArgumentSpec::new(Arc::new(StubParser)))
    }

    /// Compile-time proof of the thread-safety contract: trees,
    /// states, and spec handles are `Send + Sync`, so a tree parsed on one thread can be
    /// handed to another.
    #[test]
    fn trees_states_and_specs_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NodeTree<PlainLang>>();
        assert_send_sync::<ParsingState<PlainLang>>();
        assert_send_sync::<Arc<dyn CallableSpec<PlainLang>>>();
    }

    /// Builds the running example: `x\frac{a}{b} % note` as
    /// List [ Chars"x", Callable\frac(Group(Chars"a"), Group(Chars"b")), Chars" ",
    /// Comment" note" ]. The callable's post-space is the trigger token's own —
    /// empty here, `{` follows the name directly — so the space before the comment is
    /// sibling content (whitespace invariant 3).
    fn example_tree() -> NodeTree<PlainLang> {
        let source: Arc<Source> = Arc::new(Source::new(r"x\frac{a}{b} % note"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();

        let x = b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![]).unwrap();

        let a_chars =
            b.add(NodeKind::chars(Span::new(7, 8)), spanned(&source, 7..8), st.clone(), vec![]).unwrap();
        let a_group = b.add(
            NodeKind::group(brace_group(6..7, 8..9)),
            spanned(&source, 6..9),
            st.clone(),
            vec![a_chars],
        ).unwrap();
        let b_chars =
            b.add(NodeKind::chars(Span::new(10, 11)), spanned(&source, 10..11), st.clone(), vec![]).unwrap();
        let b_group = b.add(
            NodeKind::group(brace_group(9..10, 11..12)),
            spanned(&source, 9..12),
            st.clone(),
            vec![b_chars],
        ).unwrap();

        let arg_specs = [brace_arg_spec(), brace_arg_spec()];
        let spec: Arc<dyn CallableSpec<PlainLang>> =
            Arc::new(StdCallableSpec::new(arg_specs.to_vec()));
        let frac = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "frac".into(),
                spec,
                arguments: vec![
                    ParsedArgument::provided(
                        arg_specs[0].clone(),
                        ChildRegion::new(0..1, ContentNodes::InChildrenOf(a_group, 0..1)),
                    ),
                    ParsedArgument::provided(
                        arg_specs[1].clone(),
                        ChildRegion::new(1..2, ContentNodes::InChildrenOf(b_group, 0..1)),
                    ),
                ]
                .into(),
                slots: ParsedSlots::empty(),
                // The trigger token's own syntactic post-space: empty (`{` follows).
                post_space: TextContent::Spanned(Span::empty(6)),
                ext: (),
            }),
            spanned(&source, 1..12),
            st.clone(),
            vec![a_group, b_group],
        ).unwrap();

        let ws =
            b.add(NodeKind::chars(Span::new(12, 13)), spanned(&source, 12..13), st.clone(), vec![]).unwrap();
        let comment = b.add(
            // start "%" + content " note" + empty post_space (end of input).
            NodeKind::comment(Span::new(13, 14), Span::new(14, 19), Span::empty(19)),
            spanned(&source, 13..19),
            st.clone(),
            vec![],
        ).unwrap();

        let root = b.add(
            NodeKind::list(),
            SourceSpan::entire(&source),
            st.clone(),
            vec![x, frac, ws, comment],
        ).unwrap();
        b.finish(root).unwrap()
    }

    #[test]
    fn example_tree_passes_the_invariant_checker() {
        // Exercises the checker's callable path (children-block contiguity, post-space
        // suffix, region tiling, InChildrenOf content designations) ahead of 6.4's
        // parser-produced callables.
        check_tree_invariants(&example_tree());
    }

    #[test]
    fn tree_structure_and_accessors() {
        let tree = example_tree();
        let root = tree.root();
        assert!(root.is_list());
        assert_eq!(root.child_count(), 4);
        assert_eq!(tree.node_count(), 9);

        let x = root.child(0).unwrap();
        assert_eq!(x.chars(), Some("x"));
        assert_eq!(x.span_content(), "x");
        assert!(x.comment().is_none());

        let frac = root.child(1).unwrap();
        assert!(frac.is_callable());
        assert_eq!(frac.name(), Some("frac"));
        assert_eq!(frac.callable_type(), Some(CT_MACRO));
        assert_eq!(frac.post_space(), Some(""));
        assert_eq!(frac.span_content(), r"\frac{a}{b}");
        assert!(frac.spec().is_some());

        let comment = root.child(3).unwrap();
        assert_eq!(comment.comment(), Some(" note"));
        assert_eq!(comment.comment_start(), Some("%"));
        assert_eq!(comment.comment_post_space(), Some(""));
        assert!(comment.chars().is_none());
        assert!(x.comment_start().is_none());
        assert!(x.comment_post_space().is_none());

        assert!(root.child(4).is_none());
        // Out-of-range must hold for indices past u32 too: `1 << 32` truncates to 0
        // under an `as u32` cast and would wrongly return child 0. (checked_shl keeps
        // this compilable on 32-bit targets, where such indices don't exist.)
        if let Some(big) = 1usize.checked_shl(32) {
            assert!(root.child(big).is_none());
        }
        assert!(root.child(usize::MAX).is_none());
        // children() iterates in order:
        let kinds: Vec<bool> = root.children().iter().map(|c| c.is_callable()).collect();
        assert_eq!(kinds, [false, true, false, false]);
    }

    #[test]
    fn argument_access() {
        let tree = example_tree();
        let frac = tree.root().child(1).unwrap();

        // Region nodes: the argument's full syntactic extent (here just the group).
        let region0: Vec<_> = frac.argument_nodes(0).unwrap().iter().collect();
        assert_eq!(region0.len(), 1);
        let arg0 = region0[0];
        assert!(arg0.is_group());
        assert_eq!(arg0.group_type(), Some(GT_BRACE));
        assert_eq!(arg0.group_delimiters(), Some(("{", "}")));
        assert_eq!(arg0.span_content(), "{a}");

        // Content nodes: what the parser designated — the group's children, braces
        // excluded; read back as a plain node range, no unwrap heuristics.
        let content0: Vec<_> = frac.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content0.len(), 1);
        assert_eq!(content0[0].chars(), Some("a"));

        let args = frac.arguments().unwrap();
        // Resolved records are global node-index ranges (the NodeData.children
        // coordinate system), readable through NodeTree::nodes_in:
        let region1 = args.get(1).unwrap().region.as_ref().unwrap();
        assert!(region1.is_resolved());
        assert_eq!(tree.nodes_in(region1.children()).next().unwrap().span_content(), "{b}");
        // The content parent is the group node (delimiter queries, empty-content anchor):
        assert_eq!(tree.node(region1.content_parent()).span_content(), "{b}");

        assert!(frac.argument_nodes(2).is_none());
        assert_eq!(args.len(), 2);
        assert!(args.iter().all(|arg| arg.is_provided()));
        // Every parsed argument knows the spec it was parsed against:
        assert!(args.iter().all(|arg| Arc::strong_count(&arg.spec) >= 2));

        // Non-callables answer None everywhere:
        let x = tree.root().child(0).unwrap();
        assert!(x.argument_nodes(0).is_none());
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
            b.add(NodeKind::chars(Span::new(8, 9)), spanned(&source, 8..9), st.clone(), vec![]).unwrap();
        let t = b.add(NodeKind::chars(Span::new(10, 11)), spanned(&source, 10..11), st.clone(), vec![]).unwrap();
        let title = b.add(
            NodeKind::group(brace_group(9..10, 11..12)),
            spanned(&source, 9..12),
            st.clone(),
            vec![t],
        ).unwrap();

        let star_spec: Arc<ArgumentSpec<PlainLang>> =
            Arc::new(ArgumentSpec::new(Arc::new(StubParser)).named("star"));
        let placement_spec: Arc<ArgumentSpec<PlainLang>> =
            Arc::new(ArgumentSpec::new(Arc::new(StubParser)).named("placement"));
        let title_spec: Arc<ArgumentSpec<PlainLang>> =
            Arc::new(ArgumentSpec::new(Arc::new(StubParser)).named("title"));
        let spec: Arc<dyn CallableSpec<PlainLang>> = Arc::new(StdCallableSpec::new(vec![
            star_spec.clone(),
            placement_spec.clone(),
            title_spec.clone(),
        ]));
        let section = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "section".into(),
                spec,
                arguments: vec![
                    ParsedArgument::provided(star_spec, ChildRegion::single(0)),
                    ParsedArgument::absent(placement_spec),
                    ParsedArgument::provided(
                        title_spec,
                        ChildRegion::new(1..2, ContentNodes::InChildrenOf(title, 0..1)),
                    ),
                ]
                .into(),
                slots: ParsedSlots::empty(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            SourceSpan::entire(&source),
            st.clone(),
            vec![star, title],
        ).unwrap();
        let tree = b.finish(section).unwrap();

        let node = tree.root();
        // The provided marker is an ordinary Chars child node and counts as its own
        // content (star-as-content convention); the absent optional has an entry but no
        // region — it consumed nothing.
        assert_eq!(node.argument_content_nodes(0).unwrap().iter().next().unwrap().chars(), Some("*"));
        assert!(node.argument_nodes(1).is_none());
        assert_eq!(node.argument_nodes(2).unwrap().iter().next().unwrap().span_content(), "{t}");

        let args = node.arguments().unwrap();
        assert!(args.get(0).unwrap().is_provided());
        assert!(!args.get(1).unwrap().is_provided());
        assert!(args.get(1).unwrap().region.is_none());

        // Region-level content (the marker): the content parent is the callable itself.
        let star_region = args.get(0).unwrap().region.as_ref().unwrap();
        assert_eq!(star_region.content_parent(), node.id());
        assert_eq!(star_region.children(), star_region.content_range());

        // By-name access — absent arguments keep their spec, so "absent" and "no such
        // argument" stay distinguishable:
        let title_region = args.get_named("title").unwrap().region.as_ref().unwrap();
        assert_eq!(
            tree.nodes_in(title_region.children()).next().unwrap().span_content(),
            "{t}"
        );
        assert!(!args.get_named("placement").unwrap().is_provided());
        assert!(args.get_named("nonsense").is_none());
    }

    #[test]
    fn slots_and_body() {
        // An environment-shaped callable: one body slot (a List child after no args).
        let source: Arc<Source> = Arc::new(Source::new(r"\begin{it}hi\end{it}"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();

        let hi = b.add(NodeKind::chars(Span::new(10, 12)), spanned(&source, 10..12), st.clone(), vec![]).unwrap();
        let body = b.add(NodeKind::list(), spanned(&source, 10..12), st.clone(), vec![hi]).unwrap();

        let spec: Arc<dyn CallableSpec<PlainLang>> =
            Arc::new(StdCallableSpec::default());
        let env = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_ENVIRONMENT,
                name: "it".into(),
                spec,
                arguments: ParsedArguments::empty(),
                // The slot record is minted by the driving parser and carries its own
                // name (record-level slots, July 2026 slots session).
                slots: vec![ParsedSlot::named(
                    "body",
                    ChildRegion::new(0..1, ContentNodes::InChildrenOf(body, 0..1)),
                )]
                .into(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            SourceSpan::entire(&source),
            st.clone(),
            vec![body],
        ).unwrap();
        let tree = b.finish(env).unwrap();

        let node = tree.root();
        // The wrapper node: the body `List`, exposed as the slot's content parent.
        let body = node.slot_content_parent(0).unwrap();
        assert!(body.is_list());
        assert_eq!(body.child_count(), 1);
        assert_eq!(body.child(0).unwrap().chars(), Some("hi"));
        assert!(node.slot_content_parent(1).is_none());
        // Slot content reads as a plain node range — the body List's children;
        // `body()` is the same nodes for slot 0:
        let content: Vec<_> = node.slot_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].chars(), Some("hi"));
        let body_nodes: Vec<_> = node.body().unwrap().iter().collect();
        assert_eq!(body_nodes.len(), 1);
        assert_eq!(body_nodes[0].id(), content[0].id());
        let slots = node.slots().unwrap();
        assert_eq!(slots.len(), 1);
        // Global layout: env = 0, body List = 1, "hi" chars = 2.
        assert_eq!(slots.get_named("body").unwrap().region.children(), 1..2);
        assert_eq!(slots.get_named("body").unwrap().region.content_range(), 2..3);
    }

    /// A slot whose content sits directly among the callable's children
    /// (`ContentNodes::InRegion`): there is no wrapper node, and `slot_content_parent`
    /// reports that as `None` — never the callable itself, which would send naive
    /// recursive walkers into a loop — while the content accessors work unchanged.
    #[test]
    fn region_level_slot_content_has_no_content_parent() {
        let source: Arc<Source> = Arc::new(Source::new("hi"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();

        let hi = b.add(NodeKind::chars(Span::new(0, 2)), spanned(&source, 0..2), st.clone(), vec![]).unwrap();
        let spec: Arc<dyn CallableSpec<PlainLang>> =
            Arc::new(StdCallableSpec::default());
        let env = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_ENVIRONMENT,
                name: "it".into(),
                spec,
                arguments: ParsedArguments::empty(),
                slots: vec![ParsedSlot::named("body", ChildRegion::single(0))].into(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            SourceSpan::entire(&source),
            st.clone(),
            vec![hi],
        ).unwrap();
        let tree = b.finish(env).unwrap();

        let node = tree.root();
        // The record itself still anchors the content at the callable:
        let region = &node.slots().unwrap().get(0).unwrap().region;
        assert_eq!(region.content_parent(), node.id());
        // ... but the read API reports "no wrapper node" instead of the callable:
        assert!(node.slot_content_parent(0).is_none());
        let body: Vec<_> = node.body().unwrap().iter().collect();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].chars(), Some("hi"));
    }

    /// Every id carries its tree layout's tag ([`TreeTag`]), in every build: using an
    /// id minted by one tree on another trips [`NodeTree::node`]'s own-tree assertion
    /// instead of silently resolving to an unrelated node.
    #[test]
    #[should_panic(expected = "does not belong")]
    fn cross_tree_node_id_is_caught() {
        let tree_a = example_tree();
        let tree_b = example_tree();
        let id_from_a = tree_a.root().child(0).unwrap().id();
        let _ = tree_b.node(id_from_a); // in range for tree_b, but foreign
    }

    /// The tag participates in id identity (`Eq`/`Hash`): same-index ids of two
    /// layouts are different values, so one map can key ids from several trees.
    #[test]
    fn tree_tags_participate_in_node_id_identity() {
        let tree_a = example_tree();
        let tree_b = example_tree();
        let a0 = tree_a.root().id();
        let b0 = tree_b.root().id();
        assert_eq!(a0.index(), b0.index());
        assert_ne!(a0, b0); // the tag distinguishes them
        assert_eq!(a0.tree_tag(), tree_a.root().child(1).unwrap().id().tree_tag());

        let mut map = hashbrown::HashMap::new();
        map.insert(a0, "a");
        map.insert(b0, "b");
        assert_eq!(map.len(), 2);
        assert_eq!(map[&a0], "a");
        assert_eq!(map[&b0], "b");
    }

    /// `get()` rejects an in-range foreign id in **every** build (the tag check is
    /// no longer debug-only), and layout-preserving copies (`clone`, `materialize`,
    /// `annotate`) share the tag — their ids are interchangeable.
    #[test]
    fn get_rejects_foreign_ids_and_copies_share_the_tag() {
        let tree_a = example_tree();
        let tree_b = example_tree(); // same shape: every id is in range for both
        let id_from_a = tree_a.root().child(1).unwrap().id();
        assert!(tree_a.get(id_from_a).is_some());
        assert!(tree_b.get(id_from_a).is_none());

        let cloned = tree_a.clone();
        let materialized = tree_a.materialize();
        let annotated = tree_a.annotate(|node| node.id().index());
        assert!(cloned.get(id_from_a).is_some());
        assert!(materialized.get(id_from_a).is_some());
        assert_eq!(annotated.get(id_from_a).unwrap().annotation(), &id_from_a.index());
    }

    /// `annotate` is zero-copy over the layout: the stages share the frozen core
    /// (`Arc` identity) and the input tree is untouched; the callback runs in
    /// storage order.
    #[test]
    fn annotate_shares_the_core_and_runs_in_storage_order() {
        let tree = example_tree();
        let mut seen: Vec<usize> = Vec::new();
        let annotated = tree.annotate(|node| {
            seen.push(node.id().index());
            node.span_content().len()
        });
        // Storage order = 0..n in index order (breadth-first layout).
        assert_eq!(seen, (0..tree.node_count()).collect::<Vec<_>>());
        // Same core, same tag; only the annotation vector is new.
        assert!(Arc::ptr_eq(&tree.core, &annotated.core));
        assert_eq!(annotated.annotations().len(), tree.node_count());
        // The input tree is untouched (still `A = ()`).
        assert_eq!(tree.annotations().len(), tree.node_count());
        let x = annotated.root().child(0).unwrap();
        assert_eq!(*x.annotation(), 1); // "x"
        // Re-annotation over the annotated stage reads the current annotations.
        let doubled = annotated.annotate(|node| node.annotation() * 2);
        assert_eq!(*doubled.node(x.id()).annotation(), 2);
    }

    /// Noise between arguments: `\frac %h␊ {a}{b}` — the comment and the surrounding
    /// whitespace are ordinary child nodes inside the first argument's region, out of
    /// the way of the designated content.
    #[test]
    fn argument_regions_hold_noise_nodes() {
        let source: Arc<Source> = Arc::new(Source::new("\\frac %h\n {a}{b}"));
        // \frac=0..5  " "=5..6  %h␊=6..9 (content "h"=7..8)  " "=9..10  {a}=10..13  {b}=13..16
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();

        let ws1 = b.add(NodeKind::chars(Span::new(5, 6)), spanned(&source, 5..6), st.clone(), vec![]).unwrap();
        let com = b.add(
            // start "%" + content "h" + post_space "\n" (the node's span covers all three).
            NodeKind::comment(Span::new(6, 7), Span::new(7, 8), Span::new(8, 9)),
            spanned(&source, 6..9),
            st.clone(),
            vec![],
        ).unwrap();
        let ws2 = b.add(NodeKind::chars(Span::new(9, 10)), spanned(&source, 9..10), st.clone(), vec![]).unwrap();
        let a = b.add(NodeKind::chars(Span::new(11, 12)), spanned(&source, 11..12), st.clone(), vec![]).unwrap();
        let a_group = b.add(
            NodeKind::group(brace_group(10..11, 12..13)),
            spanned(&source, 10..13),
            st.clone(),
            vec![a],
        ).unwrap();
        let bb = b.add(NodeKind::chars(Span::new(14, 15)), spanned(&source, 14..15), st.clone(), vec![]).unwrap();
        let b_group = b.add(
            NodeKind::group(brace_group(13..14, 15..16)),
            spanned(&source, 13..16),
            st.clone(),
            vec![bb],
        ).unwrap();

        let arg_specs = [brace_arg_spec(), brace_arg_spec()];
        let spec: Arc<dyn CallableSpec<PlainLang>> =
            Arc::new(StdCallableSpec::new(arg_specs.to_vec()));
        let frac = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "frac".into(),
                spec,
                arguments: vec![
                    ParsedArgument::provided(
                        arg_specs[0].clone(),
                        ChildRegion::new(0..4, ContentNodes::InChildrenOf(a_group, 0..1)),
                    ),
                    ParsedArgument::provided(
                        arg_specs[1].clone(),
                        ChildRegion::new(4..5, ContentNodes::InChildrenOf(b_group, 0..1)),
                    ),
                ]
                .into(),
                slots: ParsedSlots::empty(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            SourceSpan::entire(&source),
            st.clone(),
            vec![ws1, com, ws2, a_group, b_group],
        ).unwrap();
        let tree = b.finish(frac).unwrap();

        let frac = tree.root();
        let region0: Vec<_> = frac.argument_nodes(0).unwrap().iter().collect();
        assert_eq!(region0.len(), 4);
        assert_eq!(region0[0].chars(), Some(" ")); // whitespace-only Chars node
        assert!(region0[1].is_comment());
        assert_eq!(region0[1].comment(), Some("h"));
        assert!(region0[3].is_group());
        // …while the content is undisturbed by the noise:
        let content0: Vec<_> = frac.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content0.len(), 1);
        assert_eq!(content0[0].chars(), Some("a"));
        // The second argument is unaffected by the first one's noise:
        assert_eq!(frac.argument_content_nodes(1).unwrap().iter().next().unwrap().chars(), Some("b"));
        // The regions tile the child list: recomposing the children in order
        // reproduces the arguments' text byte-for-byte (partition invariant).
        let all: String = frac.children().iter().map(|c| c.span_content()).collect();
        assert_eq!(all, " %h\n {a}{b}");
    }

    /// Content may be designated arbitrarily deep: `\m[{x}]` protects `]`-containing
    /// content behind an inner brace group — the content nodes are the *inner* group's
    /// children. The parser says what the content is; there is no unwrap-double-group
    /// heuristic (pylatexenc's hack).
    #[test]
    fn content_designation_reaches_into_nested_groups() {
        const GT_BRACKET: u32 = 1;
        let source: Arc<Source> = Arc::new(Source::new("\\m[{x}]"));
        // \m=0..2  [=2..3  {=3..4  x=4..5  }=5..6  ]=6..7
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();

        let x = b.add(NodeKind::chars(Span::new(4, 5)), spanned(&source, 4..5), st.clone(), vec![]).unwrap();
        let inner = b.add(
            NodeKind::group(brace_group(3..4, 5..6)),
            spanned(&source, 3..6),
            st.clone(),
            vec![x],
        ).unwrap();
        let outer = b.add(
            NodeKind::group(GroupData::new(
                GT_BRACKET,
                TextContent::Spanned(Span::new(2, 3)),
                TextContent::Spanned(Span::new(6, 7)),
            )),
            spanned(&source, 2..7),
            st.clone(),
            vec![inner],
        ).unwrap();

        let arg_spec = brace_arg_spec();
        let spec: Arc<dyn CallableSpec<PlainLang>> =
            Arc::new(StdCallableSpec::new(vec![arg_spec.clone()]));
        let m = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "m".into(),
                spec,
                arguments: vec![ParsedArgument::provided(
                    arg_spec,
                    ChildRegion::new(0..1, ContentNodes::InChildrenOf(inner, 0..1)),
                )]
                .into(),
                slots: ParsedSlots::empty(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            SourceSpan::entire(&source),
            st.clone(),
            vec![outer],
        ).unwrap();
        let tree = b.finish(m).unwrap();

        let m = tree.root();
        let region: Vec<_> = m.argument_nodes(0).unwrap().iter().collect();
        assert_eq!(region.len(), 1);
        assert_eq!(region[0].span_content(), "[{x}]");
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].chars(), Some("x"));
        // The content parent is the inner group — a descendant of, not a member of,
        // the region:
        let record = m.arguments().unwrap().get(0).unwrap().region.clone().unwrap();
        assert_eq!(tree.node(record.content_parent()).span_content(), "{x}");
    }

    /// Empty content stays anchored: `\m{}` — the content range is empty, but its
    /// parent (the group) still answers "where content would go".
    #[test]
    fn empty_content_is_anchored_by_its_parent() {
        let source: Arc<Source> = Arc::new(Source::new("\\m{}"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let group = b.add(
            NodeKind::group(brace_group(2..3, 3..4)),
            spanned(&source, 2..4),
            st.clone(),
            vec![],
        ).unwrap();
        let arg_spec = brace_arg_spec();
        let spec: Arc<dyn CallableSpec<PlainLang>> =
            Arc::new(StdCallableSpec::new(vec![arg_spec.clone()]));
        let m = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "m".into(),
                spec,
                arguments: vec![ParsedArgument::provided(
                    arg_spec,
                    ChildRegion::new(0..1, ContentNodes::InChildrenOf(group, 0..0)),
                )]
                .into(),
                slots: ParsedSlots::empty(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            SourceSpan::entire(&source),
            st.clone(),
            vec![group],
        ).unwrap();
        let tree = b.finish(m).unwrap();

        let record = tree.root().arguments().unwrap().get(0).unwrap().region.clone().unwrap();
        assert!(record.content_range().is_empty());
        assert_eq!(tree.node(record.content_parent()).span_content(), "{}");
        assert_eq!(tree.root().argument_content_nodes(0).unwrap().iter().count(), 0);
    }

    // --- builder contract violations around regions -----------------------------------
    //
    // Contract violations return `NodeBuildError` — extension-implementation bugs must
    // never panic core code (the panic policy).

    /// Helper: a one-argument callable staged over the given children with the given
    /// region record (drives the builder's region checks).
    fn stage_callable_with_arg(
        b: &mut NodeTreeBuilder<PlainLang>,
        source: &Arc<Source>,
        st: &Arc<ParsingState<PlainLang>>,
        args: Vec<ParsedArgument<PlainLang>>,
        children: Vec<BuildId>,
    ) -> Result<BuildId, NodeBuildError> {
        let specs: Vec<_> = args.iter().map(|a| a.spec.clone()).collect();
        let spec: Arc<dyn CallableSpec<PlainLang>> = Arc::new(StdCallableSpec::new(specs));
        b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "m".into(),
                spec,
                arguments: args.into(),
                slots: ParsedSlots::empty(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            SourceSpan::entire(source),
            st.clone(),
            children,
        )
    }

    #[test]
    fn region_out_of_bounds_errors() {
        let source: Arc<Source> = Arc::new(Source::new("\\m"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let args = vec![ParsedArgument::provided(brace_arg_spec(), ChildRegion::single(0))];
        let result = stage_callable_with_arg(&mut b, &source, &st, args, vec![]);
        assert_eq!(
            result,
            Err(NodeBuildError::RegionOutOfBounds { region: 0..1, n_children: 0 })
        );
    }

    #[test]
    fn overlapping_regions_error() {
        let source: Arc<Source> = Arc::new(Source::new("\\m{}"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let group = b.add(
            NodeKind::group(brace_group(2..3, 3..4)),
            spanned(&source, 2..4),
            st.clone(),
            vec![],
        ).unwrap();
        let args = vec![
            ParsedArgument::provided(brace_arg_spec(), ChildRegion::single(0)),
            ParsedArgument::provided(brace_arg_spec(), ChildRegion::single(0)),
        ];
        let result = stage_callable_with_arg(&mut b, &source, &st, args, vec![group]);
        assert_eq!(
            result,
            Err(NodeBuildError::RegionNotTiling { region: 0..1, expected_start: 1 })
        );
    }

    #[test]
    fn region_gaps_error() {
        let source: Arc<Source> = Arc::new(Source::new("\\m{}{}"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let g1 = b.add(
            NodeKind::group(brace_group(2..3, 3..4)),
            spanned(&source, 2..4),
            st.clone(),
            vec![],
        ).unwrap();
        let g2 = b.add(
            NodeKind::group(brace_group(4..5, 5..6)),
            spanned(&source, 4..6),
            st.clone(),
            vec![],
        ).unwrap();
        // One argument claiming only the second child: the first belongs to no region.
        let args = vec![ParsedArgument::provided(brace_arg_spec(), ChildRegion::single(1))];
        let result = stage_callable_with_arg(&mut b, &source, &st, args, vec![g1, g2]);
        assert_eq!(
            result,
            Err(NodeBuildError::RegionNotTiling { region: 1..2, expected_start: 0 })
        );
    }

    #[test]
    fn trailing_children_outside_regions_error() {
        let source: Arc<Source> = Arc::new(Source::new("\\m{}{}"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let g1 = b.add(
            NodeKind::group(brace_group(2..3, 3..4)),
            spanned(&source, 2..4),
            st.clone(),
            vec![],
        ).unwrap();
        let g2 = b.add(
            NodeKind::group(brace_group(4..5, 5..6)),
            spanned(&source, 4..6),
            st.clone(),
            vec![],
        ).unwrap();
        // One argument claiming only the first child: the second belongs to no region.
        let args = vec![ParsedArgument::provided(brace_arg_spec(), ChildRegion::single(0))];
        let result = stage_callable_with_arg(&mut b, &source, &st, args, vec![g1, g2]);
        assert_eq!(result, Err(NodeBuildError::ChildrenNotInRegions { unassigned: 1..2 }));
    }

    #[test]
    fn dangling_content_parent_errors() {
        let source: Arc<Source> = Arc::new(Source::new("\\m{}"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let group = b.add(
            NodeKind::group(brace_group(2..3, 3..4)),
            spanned(&source, 2..4),
            st.clone(),
            vec![],
        ).unwrap();
        // Content designated inside a staged node that never becomes part of the tree:
        let stray = b.add(NodeKind::list(), spanned(&source, 0..0), st.clone(), vec![]).unwrap();
        let args = vec![ParsedArgument::provided(
            brace_arg_spec(),
            ChildRegion::new(0..1, ContentNodes::InChildrenOf(stray, 0..0)),
        )];
        let m = stage_callable_with_arg(&mut b, &source, &st, args, vec![group]).unwrap();
        assert_eq!(
            b.finish(m).unwrap_err(),
            NodeBuildError::ContentParentUnreachable { parent: stray }
        );
    }

    #[test]
    fn content_parent_outside_its_region_errors() {
        let source: Arc<Source> = Arc::new(Source::new("\\m{}{}"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let g1 = b.add(
            NodeKind::group(brace_group(2..3, 3..4)),
            spanned(&source, 2..4),
            st.clone(),
            vec![],
        ).unwrap();
        let g2 = b.add(
            NodeKind::group(brace_group(4..5, 5..6)),
            spanned(&source, 4..6),
            st.clone(),
            vec![],
        ).unwrap();
        // Argument 0's content designated inside argument 1's group:
        let args = vec![
            ParsedArgument::provided(
                brace_arg_spec(),
                ChildRegion::new(0..1, ContentNodes::InChildrenOf(g2, 0..0)),
            ),
            ParsedArgument::provided(
                brace_arg_spec(),
                ChildRegion::new(1..2, ContentNodes::InChildrenOf(g2, 0..0)),
            ),
        ];
        let m = stage_callable_with_arg(&mut b, &source, &st, args, vec![g1, g2]).unwrap();
        assert_eq!(
            b.finish(m).unwrap_err(),
            NodeBuildError::ContentParentOutsideRegion { parent: g2 }
        );
    }

    #[test]
    fn restaging_resolved_records_errors() {
        let tree = example_tree();
        let frac = tree.root().child(1).unwrap();
        // Records from a finished tree hold that tree's node-index ranges; staging them
        // into a new builder is a caller bug (the two-phase record contract).
        let data = frac.callable().unwrap().clone();
        let source: Arc<Source> = Arc::new(Source::new("x"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::<PlainLang>::new();
        let result = b.add(NodeKind::callable(data), SourceSpan::entire(&source), st, vec![]);
        assert_eq!(result, Err(NodeBuildError::RegionAlreadyResolved));
    }

    #[test]
    fn sibling_ranges_are_contiguous_and_flat() {
        let tree = example_tree();
        // Every node's children ids are a contiguous ascending run, and each non-root
        // node is the child of exactly one node.
        let mut seen = vec![0usize; tree.node_count()];
        for node in tree.iter_storage_order() {
            let ids: Vec<usize> = node.children().iter().map(|c| c.id().index()).collect();
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
            b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![]).unwrap();
        let kept = b.add(NodeKind::chars(Span::new(1, 2)), spanned(&source, 1..2), st.clone(), vec![]).unwrap();
        let root = b.add(NodeKind::list(), SourceSpan::entire(&source), st, vec![kept]).unwrap();
        let tree = b.finish(root).unwrap();
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
        assert_eq!(root.child(1).unwrap().post_space(), Some(""));
        assert_eq!(root.child(2).unwrap().chars(), Some(" "));
        assert_eq!(root.child(3).unwrap().comment(), Some(" note"));
        assert_eq!(root.child(3).unwrap().comment_start(), Some("%"));
        let arg0 = root.child(1).unwrap().argument_nodes(0).unwrap().iter().next().unwrap();
        assert_eq!(arg0.group_delimiters(), Some(("{", "}")));
        // …but stored owned:
        for node in owned.iter_storage_order() {
            match node.kind() {
                NodeKind::Chars { content, .. } => assert!(content.is_owned()),
                NodeKind::Comment { content, start, post_space, .. } => {
                    assert!(content.is_owned());
                    assert!(start.is_owned());
                    assert!(post_space.is_owned());
                }
                NodeKind::Group(data) => {
                    assert!(data.open.is_owned());
                    assert!(data.close.is_owned());
                }
                NodeKind::Callable(data) => {
                    assert!(data.post_space.is_owned());
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
        assert_eq!(owned.root().child(1).unwrap().span_content(), r"\frac{a}{b}");
    }

    #[test]
    fn synthesized_groups_may_have_no_group_type() {
        // Internal synthetic groups (not produced by tokenization) carry delimiters but
        // no language group type.
        let source: Arc<Source> = Arc::new(Source::new("y"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let y = b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![]).unwrap();
        let g = b.add(
            NodeKind::group(GroupData::untyped(TextContent::from("{"), TextContent::from("}"))),
            spanned(&source, 0..1),
            st.clone(),
            vec![y],
        ).unwrap();
        let tree = b.finish(g).unwrap();

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
        type SlotExt = u8; // e.g. a derived cell/item count
    }

    #[derive(Debug, Clone, Copy)]
    struct ExtLang;
    impl Lang for ExtLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ExtBundle;
        type Driver = crate::engine::StdParseDriver;
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
        ).unwrap();
        let root = b.add(NodeKind::list(), SourceSpan::entire(&source), st, vec![y]).unwrap();
        let tree = b.finish(root).unwrap();

        let y = tree.root().child(0).unwrap();
        assert_eq!(*y.ext(), 42);
        match y.kind() {
            NodeKind::Chars { ext, .. } => assert_eq!(*ext, CharsMeta { weight: 7 }),
            _ => unreachable!(),
        }
        // Default tier-1 ext via plain add():
        assert_eq!(*tree.root().ext(), 0);
    }

    /// The slot-side symmetry of `ArgumentExt`: per-instance derived
    /// data about one content region (tabular cells, enumerate items) rides on the
    /// `ParsedSlot` record itself, not on the whole-callable ext.
    #[test]
    fn parsed_slot_carries_ext() {
        let mut slot: ParsedSlot<ExtLang> = ParsedSlot::named("body", ChildRegion::single(0));
        assert_eq!(slot.name(), Some("body"));
        assert_eq!(slot.ext, 0); // the constructors Default-initialize the ext
        slot.ext = 3;
        assert_eq!(slot.clone().ext, 3);
    }

    // --- Lang::finalize_node (the centralized finalization hook) ---------------------

    use core::sync::atomic::{AtomicUsize, Ordering};

    /// Counts every `finalize_node` run (test-global; only `FinalizeLang` tests read it,
    /// and they compare before/after within one test).
    static FINALIZE_CALLS: AtomicUsize = AtomicUsize::new(0);

    struct FinalizeExts;
    impl crate::state::NodeExtTypes for FinalizeExts {
        type NodeExt = u32; // set by the hook: number of descendants
        type CharsNodeExt = ();
        type GroupNodeExt = ();
        type CallableNodeExt = ();
        type CommentNodeExt = ();
        type ListNodeExt = ();
        type ArgumentExt = ();
        type SlotExt = ();
    }

    #[derive(Debug, Clone, Copy)]
    struct FinalizeLang;
    impl Lang for FinalizeLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = FinalizeExts;
        type Driver = crate::engine::StdParseDriver;

        fn finalize_node(
            _kind: &mut NodeKind<Self>,
            ext: &mut u32,
            _span: &SourceSpan,
            _parsing_state: &Arc<ParsingState<Self>>,
            children: &[BuildId],
            staged: &StagedNodes<'_, Self>,
        ) {
            FINALIZE_CALLS.fetch_add(1, Ordering::Relaxed);
            // Uniform per-node initialization through the staged read view: the number
            // of descendants, recomputed from the children each run (idempotent, per the
            // hook contract — transforms re-stage nodes).
            *ext = children
                .iter()
                .map(|c| staged.get(*c).expect("children are staged").ext() + 1)
                .sum();
        }
    }

    /// The hook runs for every staged node, of every kind, and its mutations land in the
    /// finished tree (it runs *before* the staging checks — no node escapes it).
    #[test]
    fn finalize_node_runs_for_every_staged_kind() {
        let source: Arc<Source> = Arc::new(Source::new("x{a}%c\n"));
        let st = state::<FinalizeLang>();
        let before = FINALIZE_CALLS.load(Ordering::Relaxed);
        let mut b = NodeTreeBuilder::new();

        let x = b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![]).unwrap();
        let a = b.add(NodeKind::chars(Span::new(2, 3)), spanned(&source, 2..3), st.clone(), vec![]).unwrap();
        let g = b.add(
            NodeKind::group(brace_group(1..2, 3..4)),
            spanned(&source, 1..4),
            st.clone(),
            vec![a],
        ).unwrap();
        let c = b.add(
            NodeKind::comment(Span::new(4, 5), Span::new(5, 6), Span::new(6, 7)),
            spanned(&source, 4..7),
            st.clone(),
            vec![],
        ).unwrap();
        let spec: Arc<dyn CallableSpec<FinalizeLang>> = Arc::new(StdCallableSpec::default());
        let m = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "m".into(),
                spec,
                arguments: ParsedArguments::empty(),
                slots: ParsedSlots::empty(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            spanned(&source, 7..7),
            st.clone(),
            vec![],
        ).unwrap();
        let root = b.add(
            NodeKind::list(),
            SourceSpan::entire(&source),
            st.clone(),
            vec![x, g, c, m],
        ).unwrap();
        let tree = b.finish(root).unwrap();

        // One run per add(), all five kinds included.
        assert_eq!(FINALIZE_CALLS.load(Ordering::Relaxed) - before, 6);
        // The hook's ext mutations (descendant counts) survived into the tree:
        assert_eq!(*tree.root().ext(), 5);
        let group = tree.root().child(1).unwrap();
        assert!(group.is_group());
        assert_eq!(*group.ext(), 1);
        assert_eq!(*tree.root().child(0).unwrap().ext(), 0);
    }

    // --- the staged read view ---------------------------------------------------------

    #[test]
    fn staged_nodes_view_reads_back_staged_data() {
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        assert!(b.staged_nodes().is_empty());

        let a = b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![]).unwrap();
        let l = b.add(NodeKind::list(), spanned(&source, 0..2), st.clone(), vec![a]).unwrap();

        let staged = b.staged_nodes();
        assert_eq!(staged.len(), 2);

        let view = staged.get(a).unwrap();
        assert_eq!(view.id(), a);
        assert!(matches!(view.kind(), NodeKind::Chars { .. }));
        assert_eq!(view.span().start(), 0);
        assert_eq!(view.span().end(), 1);
        assert!(view.children().is_empty());
        assert!(Arc::ptr_eq(view.parsing_state(), &st));
        let _uniform_ext: &() = view.ext();

        let list_view = staged.get(l).unwrap();
        assert_eq!(list_view.children(), &[a]);
        // Views are plain Copy proxies and debuggable:
        let copy = view;
        assert!(alloc::format!("{:?}", copy).contains("Chars"));
    }

    // --- Debug does not demand anything of the lang ZST itself -----------------------

    struct NoDeriveLang; // deliberately neither Clone nor Debug
    impl TrivialLang for NoDeriveLang {}

    #[test]
    fn debug_and_clone_without_lang_bounds() {
        let source: Arc<Source> = Arc::new(Source::new("z"));
        let st = state::<NoDeriveLang>();
        let mut b = NodeTreeBuilder::new();
        let z = b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![]).unwrap();
        let root = b.add(NodeKind::list(), SourceSpan::entire(&source), st, vec![z]).unwrap();
        let tree = b.finish(root).unwrap();

        let cloned = tree.clone();
        let dump = alloc::format!("{:?}", cloned);
        assert!(dump.contains("Chars"));
        let node_dump = alloc::format!("{:?}", tree.root().child(0).unwrap());
        assert!(node_dump.contains("NodeId(1@"));
    }

    #[test]
    fn a_node_cannot_have_two_parents() {
        let source: Arc<Source> = Arc::new(Source::new("w"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let w = b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![]).unwrap();
        let _l1 = b.add(NodeKind::list(), SourceSpan::entire(&source), st.clone(), vec![w]).unwrap();
        let result = b.add(NodeKind::list(), SourceSpan::entire(&source), st, vec![w]);
        assert_eq!(result.unwrap_err(), NodeBuildError::ChildAlreadyClaimed { child: w });
    }

    #[test]
    fn unstaged_children_and_roots_error() {
        let source: Arc<Source> = Arc::new(Source::new("w"));
        let st = state::<PlainLang>();
        // A foreign BuildId (minted by another builder) is an implementation bug the
        // empty builder diagnoses as not-staged — as child and as root alike.
        let mut other = NodeTreeBuilder::<PlainLang>::new();
        let foreign = other
            .add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![])
            .unwrap();

        let mut b = NodeTreeBuilder::<PlainLang>::new();
        let result =
            b.add(NodeKind::list(), SourceSpan::entire(&source), st.clone(), vec![foreign]);
        assert_eq!(result.unwrap_err(), NodeBuildError::ChildNotStaged { child: foreign });

        let b = NodeTreeBuilder::<PlainLang>::new();
        assert_eq!(
            b.finish(foreign).unwrap_err(),
            NodeBuildError::RootNotStaged { root: foreign }
        );
    }

    #[test]
    fn finishing_on_a_claimed_root_errors() {
        let source: Arc<Source> = Arc::new(Source::new("w"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let w = b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st.clone(), vec![]).unwrap();
        let _list = b.add(NodeKind::list(), SourceSpan::entire(&source), st, vec![w]).unwrap();
        assert_eq!(b.finish(w).unwrap_err(), NodeBuildError::RootClaimed { root: w });
    }

    #[test]
    fn spanned_content_outside_the_source_errors() {
        let source: Arc<Source> = Arc::new(Source::new("ab"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::<PlainLang>::new();
        // A chars payload past the source's end (previously a debug-only assertion;
        // an always-on error since the panic-policy decision).
        let result =
            b.add(NodeKind::chars(Span::new(0, 5)), SourceSpan::entire(&source), st, vec![]);
        assert_eq!(
            result.unwrap_err(),
            NodeBuildError::SpannedContentInvalid {
                what: "chars content",
                span: Span::new(0, 5),
                content_len: 2,
            }
        );
    }

    // --- NodeSlice, descendants, named accessors, subtree copy (Phase 7.8) ------------

    #[test]
    fn node_slice_basics() {
        let tree = example_tree();
        let children = tree.root().children();
        assert_eq!(children.len(), 4);
        assert!(!children.is_empty());
        assert_eq!(children.get(0).unwrap().chars(), Some("x"));
        assert_eq!(children.first().unwrap().chars(), Some("x"));
        assert!(children.last().unwrap().is_comment());
        assert!(children.get(4).is_none());

        // Direct iteration (IntoIterator), adaptor chains (iter()), and reversal.
        let mut count = 0;
        for node in children {
            let _ = node.span_content();
            count += 1;
        }
        assert_eq!(count, 4);
        let kinds: Vec<bool> = children.iter().map(|c| c.is_callable()).collect();
        assert_eq!(kinds, [false, true, false, false]);
        assert_eq!(children.iter().len(), 4); // ExactSizeIterator
        let backwards: Vec<_> = children.iter().rev().map(|c| c.is_comment()).collect();
        assert_eq!(backwards, [true, false, false, false]);

        // Slices are Copy; the range is the global node-index coordinate system.
        let copy = children;
        assert_eq!(copy.range(), children.range());
        let leaf = tree.root().child(0).unwrap();
        assert!(leaf.children().is_empty());
        assert!(leaf.children().span().is_none());
    }

    #[test]
    fn node_slice_spans_are_exact() {
        let tree = example_tree();
        let root = tree.root();
        // The whole child run covers the source.
        let all = root.children().span().unwrap();
        assert_eq!(all.range(), 0..19);
        assert_eq!(root.children().source_text(), Some(r"x\frac{a}{b} % note"));

        // Argument regions and content: exact sub-spans, straight off the nodes.
        let frac = root.child(1).unwrap();
        let region = frac.argument_nodes(0).unwrap();
        assert_eq!(region.span().unwrap().range(), 6..9);
        assert_eq!(region.source_text(), Some("{a}"));
        let content = frac.argument_content_nodes(0).unwrap();
        assert_eq!(content.span().unwrap().range(), 7..8);
        assert_eq!(content.source_text(), Some("a"));
        assert_eq!(content.span().unwrap().content(), "a");

        // Empty content (`\m{}`): no source material — None, honestly.
        let source: Arc<Source> = Arc::new(Source::new("\\m{}"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let group = b.add(
            NodeKind::group(brace_group(2..3, 3..4)),
            spanned(&source, 2..4),
            st.clone(),
            vec![],
        ).unwrap();
        let arg_spec = brace_arg_spec();
        let spec: Arc<dyn CallableSpec<PlainLang>> =
            Arc::new(StdCallableSpec::new(vec![arg_spec.clone()]));
        let m = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "m".into(),
                spec,
                arguments: vec![ParsedArgument::provided(
                    arg_spec,
                    ChildRegion::new(0..1, ContentNodes::InChildrenOf(group, 0..0)),
                )]
                .into(),
                slots: ParsedSlots::empty(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            SourceSpan::entire(&source),
            st.clone(),
            vec![group],
        ).unwrap();
        let tree = b.finish(m).unwrap();
        let content = tree.root().argument_content_nodes(0).unwrap();
        assert!(content.is_empty());
        assert!(content.span().is_none());
        assert!(content.source_text().is_none());
    }

    #[test]
    fn descendants_walk_in_document_order() {
        let tree = example_tree();
        // Storage order is breadth-first (`x`, `\frac…`, ` `, `%…`, `{a}`, `{b}`, `a`,
        // `b`); document order interleaves the argument groups where they occur.
        let doc: Vec<_> = tree.root().descendants().map(|n| n.span_content()).collect();
        assert_eq!(doc, ["x", r"\frac{a}{b}", "{a}", "a", "{b}", "b", " ", "% note"]);
        // Tree-level sugar walks from the root; subtree walks exclude the start node.
        let via_tree: Vec<_> = tree.descendants().map(|n| n.id()).collect();
        let via_root: Vec<_> = tree.root().descendants().map(|n| n.id()).collect();
        assert_eq!(via_tree, via_root);
        assert_eq!(via_tree.len(), tree.node_count() - 1);
        let frac = tree.root().child(1).unwrap();
        let sub: Vec<_> = frac.descendants().map(|n| n.span_content()).collect();
        assert_eq!(sub, ["{a}", "a", "{b}", "b"]);
        assert_eq!(tree.root().child(0).unwrap().descendants().count(), 0);
    }

    #[test]
    fn named_accessors_mirror_their_index_twins() {
        // The `\section*{t}` tree of absent_marker_and_named_arguments, accessed by name.
        let source: Arc<Source> = Arc::new(Source::new(r"\section*{t}"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let star =
            b.add(NodeKind::chars(Span::new(8, 9)), spanned(&source, 8..9), st.clone(), vec![]).unwrap();
        let t = b.add(NodeKind::chars(Span::new(10, 11)), spanned(&source, 10..11), st.clone(), vec![]).unwrap();
        let title = b.add(
            NodeKind::group(brace_group(9..10, 11..12)),
            spanned(&source, 9..12),
            st.clone(),
            vec![t],
        ).unwrap();
        let star_spec: Arc<ArgumentSpec<PlainLang>> =
            Arc::new(ArgumentSpec::new(Arc::new(StubParser)).named("star"));
        let placement_spec: Arc<ArgumentSpec<PlainLang>> =
            Arc::new(ArgumentSpec::new(Arc::new(StubParser)).named("placement"));
        let title_spec: Arc<ArgumentSpec<PlainLang>> =
            Arc::new(ArgumentSpec::new(Arc::new(StubParser)).named("title"));
        let spec: Arc<dyn CallableSpec<PlainLang>> = Arc::new(StdCallableSpec::new(vec![
            star_spec.clone(),
            placement_spec.clone(),
            title_spec.clone(),
        ]));
        let body_chars =
            b.add(NodeKind::chars(Span::new(10, 11)), spanned(&source, 10..11), st.clone(), vec![]).unwrap();
        let body = b.add(NodeKind::list(), spanned(&source, 10..11), st.clone(), vec![body_chars]).unwrap();
        let section = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "section".into(),
                spec,
                arguments: vec![
                    ParsedArgument::provided(star_spec, ChildRegion::single(0)),
                    ParsedArgument::absent(placement_spec),
                    ParsedArgument::provided(
                        title_spec,
                        ChildRegion::new(1..2, ContentNodes::InChildrenOf(title, 0..1)),
                    ),
                ]
                .into(),
                slots: vec![ParsedSlot::named(
                    "annex",
                    ChildRegion::new(2..3, ContentNodes::InChildrenOf(body, 0..1)),
                )]
                .into(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            SourceSpan::entire(&source),
            st.clone(),
            vec![star, title, body],
        ).unwrap();
        let tree = b.finish(section).unwrap();
        let node = tree.root();

        assert_eq!(node.argument_nodes_named("title").unwrap().source_text(), Some("{t}"));
        assert_eq!(
            node.argument_content_nodes_named("title").unwrap().first().unwrap().chars(),
            Some("t")
        );
        assert_eq!(
            node.argument_content_nodes_named("star").unwrap().first().unwrap().chars(),
            Some("*")
        );
        // Absent argument: entry exists, no region — the accessor answers None.
        assert!(node.argument_nodes_named("placement").is_none());
        assert!(node.argument_content_nodes_named("placement").is_none());
        // No such argument at all: also None (records distinguish, accessors do not).
        assert!(node.argument_nodes_named("nonsense").is_none());
        // Slots by name.
        assert_eq!(
            node.slot_content_nodes_named("annex").unwrap().first().unwrap().chars(),
            Some("t")
        );
        assert!(node.slot_content_nodes_named("nonsense").is_none());
        // Non-callables answer None throughout.
        let leaf = tree.node(tree.root().argument_content_nodes_named("star").unwrap().first().unwrap().id());
        assert!(leaf.argument_nodes_named("title").is_none());
    }

    #[test]
    fn copy_subtree_reproduces_structure_and_records() {
        let tree = example_tree();
        let mut b = NodeTreeBuilder::new();
        let root = super::copy::copy_subtree_into(&mut b, tree.root()).unwrap();
        let copy = b.finish(root).unwrap();

        // A pure copy is a well-formed tree: full invariants (span partition included).
        check_tree_invariants(&copy);
        assert_eq!(copy.node_count(), tree.node_count());
        // Same document order, same text, same spans (Arc-shared sources).
        let originals: Vec<_> = tree.descendants().map(|n| n.span_content()).collect();
        let copies: Vec<_> = copy.descendants().map(|n| n.span_content()).collect();
        assert_eq!(originals, copies);
        // Region records were re-staged and re-resolved for the new layout.
        let frac = copy.root().child(1).unwrap();
        assert_eq!(frac.argument_content_nodes(0).unwrap().first().unwrap().chars(), Some("a"));
        assert_eq!(frac.argument_nodes(1).unwrap().source_text(), Some("{b}"));
        // Specs and states are shared, not cloned.
        let original_spec = tree.root().child(1).unwrap().spec().unwrap();
        assert!(Arc::ptr_eq(frac.spec().unwrap(), original_spec));
        assert!(Arc::ptr_eq(
            frac.parsing_state(),
            tree.root().child(1).unwrap().parsing_state()
        ));
    }

    #[test]
    fn copy_subtree_restages_nested_content_designations() {
        // The `\m[{x}]` shape: content designated inside a *descendant* group of the
        // region node — the copy must remap the content parent through the id map.
        const GT_BRACKET: u32 = 1;
        let source: Arc<Source> = Arc::new(Source::new("\\m[{x}]"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::new();
        let x = b.add(NodeKind::chars(Span::new(4, 5)), spanned(&source, 4..5), st.clone(), vec![]).unwrap();
        let inner = b.add(
            NodeKind::group(brace_group(3..4, 5..6)),
            spanned(&source, 3..6),
            st.clone(),
            vec![x],
        ).unwrap();
        let outer = b.add(
            NodeKind::group(GroupData::new(
                GT_BRACKET,
                TextContent::Spanned(Span::new(2, 3)),
                TextContent::Spanned(Span::new(6, 7)),
            )),
            spanned(&source, 2..7),
            st.clone(),
            vec![inner],
        ).unwrap();
        let arg_spec = brace_arg_spec();
        let spec: Arc<dyn CallableSpec<PlainLang>> =
            Arc::new(StdCallableSpec::new(vec![arg_spec.clone()]));
        let m = b.add(
            NodeKind::callable(CallableData {
                callable_type: CT_MACRO,
                name: "m".into(),
                spec,
                arguments: vec![ParsedArgument::provided(
                    arg_spec,
                    ChildRegion::new(0..1, ContentNodes::InChildrenOf(inner, 0..1)),
                )]
                .into(),
                slots: ParsedSlots::empty(),
                post_space: TextContent::empty(),
                ext: (),
            }),
            SourceSpan::entire(&source),
            st.clone(),
            vec![outer],
        ).unwrap();
        let tree = b.finish(m).unwrap();

        let mut b = NodeTreeBuilder::new();
        let root = super::copy::copy_subtree_into(&mut b, tree.root()).unwrap();
        let copy = b.finish(root).unwrap();
        check_tree_invariants(&copy);
        let content = copy.root().argument_content_nodes(0).unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content.first().unwrap().chars(), Some("x"));
        let record = copy.root().arguments().unwrap().get(0).unwrap().region.clone().unwrap();
        assert_eq!(copy.node(record.content_parent()).span_content(), "{x}");
    }

    #[test]
    fn tree_get_is_the_non_panicking_node_access() {
        let tree = example_tree();
        assert!(tree.get(tree.root().id()).is_some());
        let deep = tree.iter_storage_order().last().unwrap().id();
        assert_eq!(tree.get(deep).unwrap().id(), deep);

        // An id from a different tree misses (out of range here; debug builds also
        // catch in-range foreign ids by provenance tag).
        let source: Arc<Source> = Arc::new(Source::new("w"));
        let st = state::<PlainLang>();
        let mut b = NodeTreeBuilder::<PlainLang>::new();
        let w = b.add(NodeKind::chars(Span::new(0, 1)), spanned(&source, 0..1), st, vec![]).unwrap();
        let small = b.finish(w).unwrap();
        assert!(small.get(deep).is_none());
    }
}
