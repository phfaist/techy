//! Read-only structural traversal: the **walk engine** shared with
//! [`recompose`](crate::recompose).
//!
//! [`walk`] drives a [`NodeVisitor`] over a subtree in **document order**
//! (preorder: [`enter`](NodeVisitor::enter) before the children,
//! [`exit`](NodeVisitor::exit) after them), with structural control per node
//! ([`VisitFlow`]): descend, skip the children, or stop the whole walk. This is
//! the structured sibling of the flat iterators
//! ([`descendants`](crate::core::node::NodeRef::descendants) yields the same
//! nodes but without nesting structure — legitimate for structure-free
//! queries).
//!
//! ```
//! use techy::core::{Language, ParsingState};
//! use techy::core::node::NodeRef;
//! use techy::error::Recovery;
//! use techy::latexlike::{Latexlike, LatexlikeDriver};
//! use techy::visit::{walk, VisitContext, VisitFlow};
//!
//! let language: Language<Latexlike> = Language::new(
//!     LatexlikeDriver::new(Recovery::Strict),
//!     ParsingState::lang_initial().expect("seed state"),
//! );
//! let tree = language.parse("a{b{c}}d").unwrap().tree;
//!
//! // Collect every chars node with its nesting depth. An inline closure
//! // annotates its two parameter types (fn items need no annotations).
//! let mut seen: Vec<(String, usize)> = Vec::new();
//! walk(
//!     tree.root(),
//!     &mut |node: NodeRef<'_, Latexlike>, cx: &VisitContext<'_, Latexlike>| {
//!         if let Some(text) = node.chars() {
//!             seen.push((text.to_string(), cx.depth()));
//!         }
//!         VisitFlow::Descend
//!     },
//! );
//! assert_eq!(
//!     seen,
//!     [("a".into(), 1), ("b".into(), 2), ("c".into(), 3), ("d".into(), 1)],
//! );
//! ```
//!
//! # The three-channel state discipline
//!
//! [`VisitContext`] carries **engine bookkeeping only** — depth and tree
//! access — and deliberately no user state. Consumer state lives in exactly
//! three places, none of them the context:
//!
//! - **run-spanning state** — the visitor's (or recomposer's) own `&mut self`
//!   fields;
//! - **fold accumulation** — the driver's locals and the call stack (the
//!   recompose fold composes values, [`recompose`](crate::recompose));
//! - **downward context** — the argument-threaded state `S` of a
//!   [`Recomposer`](crate::recompose::Recomposer).
//!
//! There is no fourth channel: a walk that needs *scoped* (downward) state IS
//! a [`Recomposer`](crate::recompose::Recomposer) with `Piece = ()`.
//!
//! # The walk is role-blind
//!
//! `walk` visits **everything** — children in
//! [`Attached`](crate::core::node::SlotRole::Attached) and
//! [`Hidden`](crate::core::node::SlotRole::Hidden) slot regions included, like
//! any other child (reads show reality; `Hidden` is never read-invisibility).
//! This is a deliberate contrast to recompose's
//! [`Concat`](crate::recompose::Recompose::Concat) default scope, which skips
//! both roles: the read/compose asymmetry is part of the slot-role contract
//! ([`SlotRole`]), not an accident of implementation.
//!
//! The walk recurses once per tree nesting level (at a small, constant stack
//! cost per level), and the parse-side descent-guard budget
//! ([`DescentGuard`](crate::core::DescentGuard)) does not bound it: a very
//! deep tree — hand-built through
//! [`NodeTreeBuilder`](crate::core::node::NodeTreeBuilder), or walked on a
//! thread with a smaller stack than the parse's — can still exhaust the
//! thread's stack.

use core::fmt;
use core::ops::ControlFlow;

use alloc::vec::Vec;

use crate::node::{NodeKind, NodeRef, NodeTree, SlotRole};
use crate::state::Lang;

/// The visitor's verdict about one entered node (returned from
/// [`NodeVisitor::enter`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisitFlow {
    /// Continue into this node's children (then [`exit`](NodeVisitor::exit)
    /// fires after them).
    Descend,
    /// Do not visit this node's children; the walk continues with its
    /// siblings ([`exit`](NodeVisitor::exit) still fires for the node itself).
    SkipChildren,
    /// Abort the **whole walk** immediately: no further
    /// [`enter`](NodeVisitor::enter) or [`exit`](NodeVisitor::exit) calls
    /// happen, the pending ancestors' exits included.
    Stop,
}

/// A structural visitor for [`walk`]: [`enter`](NodeVisitor::enter) is invoked
/// once per visited node in document order and steers the traversal via
/// [`VisitFlow`]; the defaulted [`exit`](NodeVisitor::exit) fires after the
/// node's (possibly skipped) children — the "closing bracket" hook that flat
/// iteration cannot provide.
///
/// For enter-only passes, any
/// `FnMut(NodeRef<'_, L, A>, &VisitContext<'_, L, A>) -> VisitFlow` closure is
/// a visitor via the blanket impl. One inference note: an **inline closure
/// must annotate its two parameter types** (the [module](self) doctest models
/// the spelling; a fully unannotated `|node, cx|` does not infer against the
/// generic visitor parameter), while a fn item needs no annotations.
///
/// The walk is **infallible** by design: the engine itself has nothing that
/// can fail, and a visitor that discovers an error condition records it in its
/// own `&mut self` state and returns [`VisitFlow::Stop`]. Deliberately **no
/// `Send`/`Sync` bounds** (the same argument as
/// [`RestageVisitor`](crate::transform::RestageVisitor): the walk runs
/// synchronously on the calling thread — a bound would demand without buying).
pub trait NodeVisitor<L: Lang, A> {
    /// Visit `node` (before its children) and steer the walk.
    fn enter(&mut self, node: NodeRef<'_, L, A>, cx: &VisitContext<'_, L, A>) -> VisitFlow;

    /// Called after `node`'s children (or right after
    /// [`enter`](NodeVisitor::enter), for
    /// [`SkipChildren`](VisitFlow::SkipChildren)); never called once the walk
    /// was [`Stop`](VisitFlow::Stop)ped. Defaults to a no-op.
    fn exit(&mut self, node: NodeRef<'_, L, A>, cx: &VisitContext<'_, L, A>) {
        let _ = (node, cx);
    }
}

/// Closures are enter-only visitors (see [`NodeVisitor`]'s docs).
impl<L: Lang, A, F> NodeVisitor<L, A> for F
where
    F: FnMut(NodeRef<'_, L, A>, &VisitContext<'_, L, A>) -> VisitFlow,
{
    fn enter(&mut self, node: NodeRef<'_, L, A>, cx: &VisitContext<'_, L, A>) -> VisitFlow {
        self(node, cx)
    }
}

/// The engine bookkeeping of one [`walk`] (or one
/// [`recompose`](crate::recompose::recompose) run), lent to every visitor
/// call. Deliberately **no user state** — see the [module docs](self) for the
/// three-channel discipline.
pub struct VisitContext<'t, L: Lang, A = ()> {
    tree: &'t NodeTree<L, A>,
    depth: usize,
}

impl<'t, L: Lang, A> VisitContext<'t, L, A> {
    /// The current nesting depth: `0` for the walk's start node, one more per
    /// descent level below it.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// The tree being walked (the start node's tree).
    pub fn tree(&self) -> &'t NodeTree<L, A> {
        self.tree
    }
}

impl<L: Lang, A> fmt::Debug for VisitContext<'_, L, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VisitContext").field("depth", &self.depth).finish()
    }
}

/// Walk the subtree rooted at `node` (the node itself included) in document
/// order, driving `visitor` — see [`NodeVisitor`] and the [module docs](self).
/// The whole tree is `walk(tree.root(), visitor)`; any other node walks just
/// its subtree, with [`depth`](VisitContext::depth) `0` at the start node.
///
/// The walk is **role-blind**: children in `Attached` and `Hidden` slot
/// regions are visited like any others (module docs).
pub fn walk<L, A, V>(node: NodeRef<'_, L, A>, visitor: &mut V)
where
    L: Lang,
    V: NodeVisitor<L, A> + ?Sized,
{
    let mut cx = VisitContext { tree: node.tree(), depth: 0 };
    let _ = drive(node, visitor, &mut cx);
}

/// One node of the walk: enter, recurse per the verdict, exit.
/// `ControlFlow::Break` = the walk was stopped.
fn drive<L, A, V>(
    node: NodeRef<'_, L, A>,
    visitor: &mut V,
    cx: &mut VisitContext<'_, L, A>,
) -> ControlFlow<()>
where
    L: Lang,
    V: NodeVisitor<L, A> + ?Sized,
{
    match visitor.enter(node, cx) {
        VisitFlow::Stop => return ControlFlow::Break(()),
        VisitFlow::SkipChildren => {}
        VisitFlow::Descend => {
            cx.depth += 1;
            // Role-blind: every child, Attached/Hidden slot regions included.
            for child in scoped_children(node, true, true) {
                drive(child, visitor, cx)?;
            }
            cx.depth -= 1;
        }
    }
    visitor.exit(node, cx);
    ControlFlow::Continue(())
}

// --- the shared descent kernel ---------------------------------------------------------

/// The one child-iteration kernel every traversal descends through — the walk
/// (role-blind: both flags `true`) and the recompose driver's `Concat`
/// lowering (the instruction's scope flags) are clients of the same kernel.
///
/// Yields `node`'s structural children in order, skipping children that lie in
/// an [`Attached`](SlotRole::Attached) slot region (unless `include_attached`)
/// or a [`Hidden`](SlotRole::Hidden) slot region (unless `include_hidden`).
/// Only callables carry slots; every other kind yields all children.
pub(crate) fn scoped_children<'t, L: Lang, A>(
    node: NodeRef<'t, L, A>,
    include_attached: bool,
    include_hidden: bool,
) -> impl Iterator<Item = NodeRef<'t, L, A>> {
    // The excluded slot regions, as global node-index ranges (regions are
    // contiguous runs of the callable's children; slots are few per node).
    let mut excluded: Vec<core::ops::Range<u32>> = Vec::new();
    if !(include_attached && include_hidden) {
        if let NodeKind::Callable(data) = node.kind() {
            for slot in data.slots.iter() {
                let skip = match slot.role {
                    SlotRole::Content => false,
                    SlotRole::Attached => !include_attached,
                    SlotRole::Hidden => !include_hidden,
                };
                if skip {
                    excluded.push(slot.region.children());
                }
            }
        }
    }
    let tree = node.tree();
    node.children()
        .range()
        .filter(move |index| !excluded.iter().any(|range| range.contains(index)))
        .map(move |index| tree.node(tree.make_id(index)))
}

#[cfg(test)]
mod tests {
    use alloc::string::{String, ToString};
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    use crate::engine::Language;
    use crate::error::Recovery;
    use crate::latexlike::{
        BodyMarker, CallableType, InvocationSyntaxData, Latexlike, LatexlikeDriver,
    };
    use crate::node::{
        CallableData, ChildRegion, ContentNodes, NodeKind, NodeRef, NodeTree,
        NodeTreeBuilder, ParsedArguments, ParsedSlot, ParsedSlots, SlotRole,
    };
    use crate::source::{Source, SourceSpan, TextContent};
    use crate::spec::StdCallableSpec;
    use crate::state::{Lang, ParsingState};

    use super::{scoped_children, walk, NodeVisitor, VisitContext, VisitFlow};

    /// Parse `input` with the plain latexlike preset (strict).
    fn parse(input: &str) -> NodeTree<Latexlike> {
        let language: Language<Latexlike> = Language::new(
            LatexlikeDriver::new(Recovery::Strict),
            ParsingState::lang_initial().expect("seed state"),
        );
        let result = language.parse(input).expect("test inputs parse cleanly");
        assert!(result.diagnostics.is_empty(), "unexpected: {:?}", result.diagnostics);
        result.tree
    }

    /// An event-logging visitor: `enter`/`exit` per node summary, with depth.
    #[derive(Default)]
    struct Log {
        events: Vec<String>,
        stop_at: Option<&'static str>,
        skip_at: Option<&'static str>,
    }

    impl NodeVisitor<Latexlike, ()> for Log {
        fn enter(
            &mut self,
            node: NodeRef<'_, Latexlike>,
            cx: &VisitContext<'_, Latexlike>,
        ) -> VisitFlow {
            let summary = node.summary();
            self.events.push(alloc::format!("enter[{}] {}", cx.depth(), summary));
            if let Some(stop) = self.stop_at {
                if summary.contains(stop) {
                    return VisitFlow::Stop;
                }
            }
            if let Some(skip) = self.skip_at {
                if summary.contains(skip) {
                    return VisitFlow::SkipChildren;
                }
            }
            VisitFlow::Descend
        }

        fn exit(&mut self, node: NodeRef<'_, Latexlike>, cx: &VisitContext<'_, Latexlike>) {
            self.events.push(alloc::format!("exit[{}] {}", cx.depth(), node.summary()));
        }
    }

    #[test]
    fn walk_is_preorder_with_paired_exits_and_depths() {
        let tree = parse("a{b}c");
        let mut log = Log::default();
        walk(tree.root(), &mut log);
        assert_eq!(
            log.events,
            [
                "enter[0] list(3)",
                "enter[1] chars(a)",
                "exit[1] chars(a)",
                "enter[1] group(Content { })",
                "enter[2] chars(b)",
                "exit[2] chars(b)",
                "exit[1] group(Content { })",
                "enter[1] chars(c)",
                "exit[1] chars(c)",
                "exit[0] list(3)",
            ]
        );
    }

    #[test]
    fn skip_children_skips_the_subtree_but_still_exits() {
        let tree = parse("a{b}c");
        let mut log = Log { skip_at: Some("group"), ..Log::default() };
        walk(tree.root(), &mut log);
        assert_eq!(
            log.events,
            [
                "enter[0] list(3)",
                "enter[1] chars(a)",
                "exit[1] chars(a)",
                "enter[1] group(Content { })",
                "exit[1] group(Content { })", // no chars(b) events
                "enter[1] chars(c)",
                "exit[1] chars(c)",
                "exit[0] list(3)",
            ]
        );
    }

    #[test]
    fn stop_aborts_the_whole_walk_without_further_events() {
        let tree = parse("a{b}c");
        let mut log = Log { stop_at: Some("chars(b)"), ..Log::default() };
        walk(tree.root(), &mut log);
        assert_eq!(
            log.events,
            [
                "enter[0] list(3)",
                "enter[1] chars(a)",
                "exit[1] chars(a)",
                "enter[1] group(Content { })",
                "enter[2] chars(b)",
                // Stop: no exits fire — not for chars(b), not for the pending
                // ancestors (group, root), and chars(c) is never entered.
            ]
        );
    }

    #[test]
    fn a_subtree_walk_starts_at_depth_zero() {
        let tree = parse("a{b{c}}");
        let group = tree.root().child(1).unwrap();
        let mut log = Log::default();
        walk(group, &mut log);
        assert_eq!(
            log.events,
            [
                "enter[0] group(Content { })",
                "enter[1] chars(b)",
                "exit[1] chars(b)",
                "enter[1] group(Content { })",
                "enter[2] chars(c)",
                "exit[2] chars(c)",
                "exit[1] group(Content { })",
                "exit[0] group(Content { })",
            ]
        );
    }

    #[test]
    fn the_closure_blanket_supports_inline_closures() {
        let tree = parse("x{y}");
        let mut names: Vec<String> = Vec::new();
        walk(
            tree.root(),
            &mut |node: NodeRef<'_, Latexlike>, _cx: &VisitContext<'_, Latexlike>| {
                if let Some(text) = node.chars() {
                    names.push(text.to_string());
                }
                VisitFlow::Descend
            },
        );
        assert_eq!(names, ["x", "y"]);
    }

    // --- slot roles: the walk is role-blind, the kernel is scope-aware ------------------

    /// A hand-staged callable with Content, Attached, and Hidden slots (the
    /// transform suite's fixture shape).
    fn three_slot_fixture() -> NodeTree<Latexlike> {
        let source = Arc::new(Source::new("stub"));
        let span = || SourceSpan::new(&source, 0..4);
        let state = Arc::new(ParsingState::<Latexlike>::lang_initial().expect("seed state"));

        let mut builder: NodeTreeBuilder<Latexlike> = NodeTreeBuilder::new();
        let chars = |builder: &mut NodeTreeBuilder<Latexlike>, text: &str| {
            let kind = NodeKind::chars(TextContent::Owned(text.into()));
            let ext = <Latexlike as Lang>::make_node_ext(
                &kind,
                &span(),
                &state,
                builder.staged_children(&[]),
            )
            .expect("mint node ext");
            builder.add(kind, span(), state.clone(), Vec::new(), ext, ()).unwrap()
        };
        let content_child = chars(&mut builder, "content");
        let attached_child = chars(&mut builder, "attached");
        let hidden_child = chars(&mut builder, "hidden");

        let slot = |offset: u32, role: SlotRole| {
            ParsedSlot::new_unnamed(
                ChildRegion::new(offset..offset + 1, ContentNodes::InRegion(0..1)),
                role,
                BodyMarker::not_body(),
            )
        };
        let kind: NodeKind<Latexlike> = NodeKind::callable(CallableData {
            callable_type: CallableType::Macro,
            name: "stub".into(),
            spec: Arc::new(StdCallableSpec::default()),
            arguments: ParsedArguments::empty(),
            slots: ParsedSlots::new(vec![
                slot(0, SlotRole::Content),
                slot(1, SlotRole::Attached),
                slot(2, SlotRole::Hidden),
            ]),
            invocation_syntax: InvocationSyntaxData::Macro {
                escape_char: '\\',
                post_space: TextContent::Owned("".into()),
            },
        });
        let children = vec![content_child, attached_child, hidden_child];
        let ext = <Latexlike as Lang>::make_node_ext(
            &kind,
            &span(),
            &state,
            builder.staged_children(&children),
        )
        .expect("mint node ext");
        let root = builder.add(kind, span(), state.clone(), children, ext, ()).unwrap();
        builder.finish(root).unwrap()
    }

    #[test]
    fn the_walk_visits_slot_children_of_every_role() {
        let tree = three_slot_fixture();
        let mut seen: Vec<String> = Vec::new();
        walk(
            tree.root(),
            &mut |node: NodeRef<'_, Latexlike>, _cx: &VisitContext<'_, Latexlike>| {
                if let Some(text) = node.chars() {
                    seen.push(text.to_string());
                }
                VisitFlow::Descend
            },
        );
        assert_eq!(seen, ["content", "attached", "hidden"]);
    }

    fn kernel_texts(tree: &NodeTree<Latexlike>, attached: bool, hidden: bool) -> Vec<String> {
        scoped_children(tree.root(), attached, hidden)
            .map(|child| child.chars().unwrap().to_string())
            .collect()
    }

    #[test]
    fn the_kernel_default_scope_skips_attached_and_hidden() {
        let tree = three_slot_fixture();
        assert_eq!(kernel_texts(&tree, false, false), ["content"]);
    }

    #[test]
    fn the_kernel_widens_per_flag() {
        let tree = three_slot_fixture();
        assert_eq!(kernel_texts(&tree, true, false), ["content", "attached"]);
        assert_eq!(kernel_texts(&tree, false, true), ["content", "hidden"]);
        assert_eq!(kernel_texts(&tree, true, true), ["content", "attached", "hidden"]);
    }

    #[test]
    fn the_kernel_takes_the_fast_path_for_non_callables() {
        // A plain group has no slots: every child, both flags irrelevant.
        let tree = parse("{ab}");
        let group = tree.root().child(0).unwrap();
        let texts: Vec<String> = scoped_children(group, false, false)
            .map(|child| child.chars().unwrap().to_string())
            .collect();
        assert_eq!(texts, ["ab"]);
    }
}
