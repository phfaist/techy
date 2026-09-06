//! Read-only traversal of a parsed node tree.
//!
//! [`TreeWalker`] drives a [`NodeVisitor`] over a subtree in document order:
//! [`enter`](NodeVisitor::enter) is called for a node before its children and
//! [`exit`](NodeVisitor::exit) after them, and the [`VisitFlow`] value returned
//! from `enter` decides what the walk does next — descend into the children,
//! skip them, or stop the whole walk.
//!
//! Use a walk when a pass needs to know how nodes are nested, or needs a hook
//! that fires after a node's children. When it needs neither, the flat iterator
//! [`descendants`](crate::core::node::NodeRef::descendants) yields the same
//! nodes without the enter/exit pairing and is simpler.
//!
//! The same traversal engine also drives [`recompose`](crate::recompose), which
//! combines a value out of the nodes it visits instead of only reading them.
//! The guide compares the tree consumers side by side:
//! [Traversing](crate::guide::node_trees#traversing-techyvisit).
//!
//! ```
//! use techy::core::{Language, ParsingState};
//! use techy::core::node::NodeRef;
//! use techy::error::Recovery;
//! use techy::latexlike::{Latexlike, LatexlikeDriver};
//! use techy::visit::{TreeWalker, VisitContext, VisitFlow};
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
//! TreeWalker::new(
//!     &mut |node: NodeRef<'_, Latexlike>, cx: &VisitContext<'_, Latexlike>| {
//!         if let Some(text) = node.chars() {
//!             seen.push((text.to_string(), cx.depth()));
//!         }
//!         VisitFlow::Descend
//!     },
//! )
//! .walk(tree.root())
//! .unwrap();
//! assert_eq!(
//!     seen,
//!     [("a".into(), 1), ("b".into(), 2), ("c".into(), 3), ("d".into(), 1)],
//! );
//! ```
//!
//! # Every node is visited the same way
//!
//! The walk visits every child of every node, and does not look at the role a
//! node plays in its parent: children in
//! [`Attached`](crate::core::node::SlotRole::Attached) and
//! [`Hidden`](crate::core::node::SlotRole::Hidden) slot regions are entered
//! like any other child. `Hidden` marks a node that recomposition leaves out by
//! default, not one that reading passes cannot see (see [`SlotRole`]).
//!
//! Recomposition differs here on purpose: the default scope of
//! [`Concat`](crate::recompose::Recompose::Concat) skips children in both of
//! those roles.
//!
//! # Nesting depth is capped
//!
//! The walk recurses once per level of nesting, so a deeply nested tree would
//! otherwise be able to exhaust the thread's stack. Every level therefore asks
//! the run's descent guard ([`StdDescentGuard`]) for permission first, and a
//! tree nested deeper than the limit allows makes
//! [`walk`](TreeWalker::walk) return
//! [`WalkError::DescentLimitExceeded`] rather than crash. Set the limit for a
//! run with
//! [`with_descent_guard_init`](TreeWalker::with_descent_guard_init).
//!
//! The limit is reached by trees the parser itself would have refused: one
//! built by hand through
//! [`NodeTreeBuilder`](crate::core::node::NodeTreeBuilder), or one parsed on a
//! thread with more stack than the thread that walks it. Shortly before
//! refusing, the guard emits a warning, which the walk reports to the visitor's
//! [`observe_descent_warning`](NodeVisitor::observe_descent_warning) method.
//!
//! # Where a visitor keeps its own state
//!
//! [`VisitContext`] gives a visitor the walk's own bookkeeping — the current
//! depth and the tree — and no state of the consumer's. State that spans the
//! run belongs in the visitor's own `&mut self` fields, which stay owned by the
//! caller and can be read back after [`walk`](TreeWalker::walk) returns.
//!
//! A pass that instead needs state handed *down* to the nodes below a given
//! node is a [`Recomposer`](crate::recompose::Recomposer) with `Piece = ()`:
//! its state parameter `S` is threaded down the tree that way.

use core::fmt;
use core::ops::ControlFlow;

use alloc::string::String;
use alloc::vec::Vec;

use crate::engine::{DescentGuard, DescentWarning, StdDescentGuard, StdDescentGuardInit};
use crate::node::{NodeKind, NodeRef, NodeTree, SlotRole};
use crate::state::Lang;

/// What a walk does after entering one node.
///
/// [`NodeVisitor::enter`] returns one of these values for every node it is
/// given, and [`TreeWalker`] continues accordingly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisitFlow {
    /// Visit this node's children next, then call
    /// [`exit`](NodeVisitor::exit) for this node, then continue with its
    /// next sibling.
    Descend,
    /// Leave this node's children unvisited: call
    /// [`exit`](NodeVisitor::exit) for this node right away, then continue
    /// with its next sibling.
    SkipChildren,
    /// End the whole walk immediately. No further
    /// [`enter`](NodeVisitor::enter) or [`exit`](NodeVisitor::exit) call
    /// happens, not even [`exit`](NodeVisitor::exit) for this node or for the
    /// ancestors already entered above it, and
    /// [`walk`](TreeWalker::walk) returns `Ok(())`.
    Stop,
}

/// The callback side of a walk: what [`TreeWalker`] calls for each node.
///
/// [`enter`](NodeVisitor::enter) is called once per visited node, in document
/// order, and its [`VisitFlow`] return value decides what the walk does next.
/// The defaulted [`exit`](NodeVisitor::exit) is called after the node's
/// children (or right after `enter`, when the children were skipped); it is the
/// hook that flat iteration over
/// [`descendants`](crate::core::node::NodeRef::descendants) cannot offer.
///
/// A pass that only needs `enter` can pass a closure instead of implementing
/// this trait: every
/// `FnMut(NodeRef<'_, L, A>, &VisitContext<'_, L, A>) -> VisitFlow` is a
/// visitor. An inline closure has to annotate both of its parameter types — a
/// bare `|node, cx|` does not infer against the generic visitor parameter — as
/// the [module-level example](self) shows; a named function needs no
/// annotations.
///
/// A visitor cannot fail: there is no error return. A visitor that finds a
/// problem records it in its own `&mut self` fields and returns
/// [`VisitFlow::Stop`]; the only way a run itself fails is the descent guard
/// refusing to go deeper ([`WalkError`]). Visitors need not be `Send` or
/// `Sync`, because a walk runs on the calling thread.
pub trait NodeVisitor<L: Lang, A> {
    /// Visit `node` (before its children) and steer the walk.
    fn enter(&mut self, node: NodeRef<'_, L, A>, cx: &VisitContext<'_, L, A>) -> VisitFlow;

    /// Visit `node` again, after its children.
    ///
    /// This is called right after [`enter`](NodeVisitor::enter) when that
    /// returned [`SkipChildren`](VisitFlow::SkipChildren), and not at all once
    /// a visitor has returned [`Stop`](VisitFlow::Stop). Defaults to doing
    /// nothing.
    fn exit(&mut self, node: NodeRef<'_, L, A>, cx: &VisitContext<'_, L, A>) {
        let _ = (node, cx);
    }

    /// Report that the walk is approaching its nesting-depth limit.
    ///
    /// The walk's descent guard allowed the descent but warned about it — under
    /// the default configuration, once half the stack budget is used (see
    /// [`StdDescentGuardInit`]). Going deeper still would make
    /// [`walk`](TreeWalker::walk) fail with
    /// [`WalkError::DescentLimitExceeded`]. Defaults to ignoring the warning.
    fn observe_descent_warning(&mut self, warning: DescentWarning) {
        let _ = warning;
    }
}

/// Any suitable closure is a visitor whose only hook is
/// [`enter`](NodeVisitor::enter).
impl<L: Lang, A, F> NodeVisitor<L, A> for F
where
    F: FnMut(NodeRef<'_, L, A>, &VisitContext<'_, L, A>) -> VisitFlow,
{
    fn enter(&mut self, node: NodeRef<'_, L, A>, cx: &VisitContext<'_, L, A>) -> VisitFlow {
        self(node, cx)
    }
}

/// The walk's own bookkeeping, passed to every [`NodeVisitor`] call.
///
/// It answers where in the tree the walk currently is — [`depth`](Self::depth)
/// and [`tree`](Self::tree) — and holds no state belonging to the visitor. A
/// visitor keeps its own state in its `&mut self` fields (see the
/// [module docs](self)). The [`recompose`](crate::recompose) driver passes the
/// same type to its callbacks.
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

    /// The tree being walked — the tree of the node
    /// [`walk`](TreeWalker::walk) was given.
    pub fn tree(&self) -> &'t NodeTree<L, A> {
        self.tree
    }
}

impl<L: Lang, A> fmt::Debug for VisitContext<'_, L, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VisitContext").field("depth", &self.depth).finish()
    }
}

/// The driver side of a walk: runs a [`NodeVisitor`] over a subtree.
///
/// Build one with [`new`](TreeWalker::new), optionally adjust the
/// nesting-depth limit with
/// [`with_descent_guard_init`](TreeWalker::with_descent_guard_init), then call
/// [`walk`](TreeWalker::walk) with the node to start from — `tree.root()` for a
/// whole tree, any other node for just its subtree.
///
/// The walker borrows the visitor mutably for the duration of the run, so
/// whatever the visitor collected is still owned by the caller and can be read
/// afterwards.
///
/// # Examples
///
/// A visitor with both hooks: `enter` records each group node with its nesting
/// depth, and `exit` closes it again.
///
/// ```
/// use techy::core::node::NodeRef;
/// use techy::core::{Language, ParsingState};
/// use techy::error::Recovery;
/// use techy::latexlike::{Latexlike, LatexlikeDriver};
/// use techy::visit::{NodeVisitor, TreeWalker, VisitContext, VisitFlow};
///
/// struct Outline {
///     lines: Vec<String>,
///     open: usize,
/// }
///
/// impl NodeVisitor<Latexlike, ()> for Outline {
///     fn enter(
///         &mut self,
///         node: NodeRef<'_, Latexlike, ()>,
///         cx: &VisitContext<'_, Latexlike, ()>,
///     ) -> VisitFlow {
///         if node.is_group() {
///             self.lines.push(format!("{}group", "  ".repeat(cx.depth())));
///             self.open += 1;
///         }
///         VisitFlow::Descend
///     }
///
///     fn exit(&mut self, node: NodeRef<'_, Latexlike, ()>, _cx: &VisitContext<'_, Latexlike, ()>) {
///         if node.is_group() {
///             self.open -= 1;
///         }
///     }
/// }
///
/// let language: Language<Latexlike> = Language::new(
///     LatexlikeDriver::new(Recovery::Strict),
///     ParsingState::lang_initial().expect("seed state"),
/// );
/// let tree = language.parse("a{b{c}}d").unwrap().tree;
///
/// let mut outline = Outline { lines: Vec::new(), open: 0 };
/// TreeWalker::new(&mut outline).walk(tree.root()).unwrap();
///
/// assert_eq!(outline.lines, ["  group", "    group"]);
/// assert_eq!(outline.open, 0); // every `enter` was matched by an `exit`
/// ```
pub struct TreeWalker<'v, V: ?Sized> {
    visitor: &'v mut V,
    descent_guard_init: StdDescentGuardInit,
}

impl<'v, V: ?Sized> TreeWalker<'v, V> {
    /// Creates a walker that will drive `visitor`.
    ///
    /// Adjust the run with the `with_*` methods, then start it with
    /// [`walk`](TreeWalker::walk).
    pub fn new(visitor: &'v mut V) -> TreeWalker<'v, V> {
        TreeWalker { visitor, descent_guard_init: StdDescentGuardInit::default() }
    }

    /// Sets how deep this run may descend.
    ///
    /// A walk costs one descent per level of nesting, and
    /// [`StdDescentGuardInit`] expresses the limit as a stack budget, as a
    /// depth limit, or as no limit at all. Without this call the run uses the
    /// guard's default, a deliberately small stack budget; a tree that exceeds
    /// the limit makes [`walk`](TreeWalker::walk) return
    /// [`WalkError::DescentLimitExceeded`].
    pub fn with_descent_guard_init(mut self, init: StdDescentGuardInit) -> TreeWalker<'v, V> {
        self.descent_guard_init = init;
        self
    }

    /// Runs the walk over the subtree rooted at `node`, `node` itself included.
    ///
    /// Nodes are visited in document order, and
    /// [`depth`](VisitContext::depth) is `0` at `node`. Pass `tree.root()` to
    /// walk a whole tree, any other node to walk just its subtree.
    ///
    /// Every child of every visited node is entered, whatever role it plays in
    /// its parent — children in `Attached` and `Hidden` slot regions included
    /// (module docs).
    ///
    /// # Errors
    ///
    /// [`WalkError::DescentLimitExceeded`] if the subtree is nested deeper than
    /// this run's descent guard allows; the walk is then abandoned partway
    /// through. A visitor that returns [`VisitFlow::Stop`] ends the walk early
    /// but is not an error: the result is `Ok(())`.
    pub fn walk<L, A>(self, node: NodeRef<'_, L, A>) -> Result<(), WalkError>
    where
        L: Lang,
        V: NodeVisitor<L, A>,
    {
        let mut guard = StdDescentGuard::init(&self.descent_guard_init);
        let mut cx = VisitContext { tree: node.tree(), depth: 0 };
        match drive(node, self.visitor, &mut cx, &mut guard) {
            ControlFlow::Break(Some(error)) => Err(error),
            _ => Ok(()),
        }
    }
}

impl<V: ?Sized> fmt::Debug for TreeWalker<'_, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TreeWalker")
            .field("descent_guard_init", &self.descent_guard_init)
            .finish_non_exhaustive()
    }
}

/// The reason a [`TreeWalker`] run failed.
///
/// A [`NodeVisitor`] has no way to report an error, so this has the single
/// variant below: a run fails only by hitting its nesting-depth limit.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WalkError {
    /// The subtree is nested deeper than the run's descent guard allows, so
    /// the guard refused to go one level further and the walk was abandoned
    /// partway through. Raise or remove the limit with
    /// [`with_descent_guard_init`](TreeWalker::with_descent_guard_init).
    DescentLimitExceeded {
        /// Which limit was hit and how to configure it.
        detail: String,
    },
}

impl fmt::Display for WalkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WalkError::DescentLimitExceeded { detail } => {
                write!(f, "walk descent limit exceeded: {}", detail)
            }
        }
    }
}

impl core::error::Error for WalkError {}

/// One node of the walk, guarded: ask the guard, enter, recurse per the
/// verdict, exit — `exit` on the guard runs for every granted descent, also
/// when a `Break` unwinds through. `Break(None)` = the visitor stopped the
/// walk; `Break(Some(_))` = the guard refused a descent.
fn drive<L, A, V>(
    node: NodeRef<'_, L, A>,
    visitor: &mut V,
    cx: &mut VisitContext<'_, L, A>,
    guard: &mut StdDescentGuard,
) -> ControlFlow<Option<WalkError>>
where
    L: Lang,
    V: NodeVisitor<L, A> + ?Sized,
{
    match guard.try_enter() {
        Err(refusal) => {
            return ControlFlow::Break(Some(WalkError::DescentLimitExceeded {
                detail: refusal.detail,
            }))
        }
        Ok(Some(warning)) => visitor.observe_descent_warning(warning),
        Ok(None) => {}
    }
    let flow = drive_entered(node, visitor, cx, guard);
    guard.exit();
    flow
}

/// The granted-descent body: enter, recurse per the verdict, exit.
fn drive_entered<L, A, V>(
    node: NodeRef<'_, L, A>,
    visitor: &mut V,
    cx: &mut VisitContext<'_, L, A>,
    guard: &mut StdDescentGuard,
) -> ControlFlow<Option<WalkError>>
where
    L: Lang,
    V: NodeVisitor<L, A> + ?Sized,
{
    match visitor.enter(node, cx) {
        VisitFlow::Stop => return ControlFlow::Break(None),
        VisitFlow::SkipChildren => {}
        VisitFlow::Descend => {
            cx.depth += 1;
            // Role-blind: every child, Attached/Hidden slot regions included.
            for child in scoped_children(node, true, true) {
                drive(child, visitor, cx, guard)?;
            }
            cx.depth -= 1;
        }
    }
    visitor.exit(node, cx);
    ControlFlow::Continue(())
}

// --- the shared descent kernel ---------------------------------------------------------

/// The single child-iteration routine every traversal descends through: the
/// walk (which passes `true` for both flags, so that every child is yielded)
/// and the recompose driver's `Concat` lowering (which passes the
/// instruction's scope flags).
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
// The fixtures mint node extensions through `Lang::make_node_ext` and hand the
// result to `NodeTreeBuilder::add`, the way the parser does. `Latexlike`'s
// extension type happens to be `()` today, which makes those bindings unit-valued;
// keeping them spelled out is what keeps the fixtures honest if the type changes.
#[allow(clippy::let_unit_value)]
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

    use super::{
        scoped_children, NodeVisitor, TreeWalker, VisitContext, VisitFlow, WalkError,
    };
    use crate::engine::{DescentWarning, StdDescentGuardInit};

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
        TreeWalker::new(&mut log).walk(tree.root()).unwrap();
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
        TreeWalker::new(&mut log).walk(tree.root()).unwrap();
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
        // A visitor Stop is an Ok outcome (the one error is the guard's refusal).
        TreeWalker::new(&mut log).walk(tree.root()).unwrap();
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
        TreeWalker::new(&mut log).walk(group).unwrap();
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
        TreeWalker::new(
            &mut |node: NodeRef<'_, Latexlike>, _cx: &VisitContext<'_, Latexlike>| {
                if let Some(text) = node.chars() {
                    names.push(text.to_string());
                }
                VisitFlow::Descend
            },
        )
        .walk(tree.root())
        .unwrap();
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
        TreeWalker::new(
            &mut |node: NodeRef<'_, Latexlike>, _cx: &VisitContext<'_, Latexlike>| {
                if let Some(text) = node.chars() {
                    seen.push(text.to_string());
                }
                VisitFlow::Descend
            },
        )
        .walk(tree.root())
        .unwrap();
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

    // --- the descent guard bounds the walk ----------------------------------------------

    /// A hand-built chain of `levels` nested untyped groups around one chars
    /// node — deeper than any parse could stage under its own guard.
    fn deep_group_chain(levels: usize) -> NodeTree<Latexlike> {
        use crate::node::GroupData;

        let source = Arc::new(Source::new("stub"));
        let span = || SourceSpan::new(&source, 0..4);
        let state = Arc::new(ParsingState::<Latexlike>::lang_initial().expect("seed state"));

        let mut builder: NodeTreeBuilder<Latexlike> = NodeTreeBuilder::new();
        let kind = NodeKind::chars(TextContent::Owned("x".into()));
        let ext = <Latexlike as Lang>::make_node_ext(
            &kind,
            &span(),
            &state,
            builder.staged_children(&[]),
        )
        .expect("mint node ext");
        let mut node = builder.add(kind, span(), state.clone(), Vec::new(), ext, ()).unwrap();
        for _ in 0..levels {
            let kind: NodeKind<Latexlike> = NodeKind::group(GroupData::untyped("{", "}"));
            let children = vec![node];
            let ext = <Latexlike as Lang>::make_node_ext(
                &kind,
                &span(),
                &state,
                builder.staged_children(&children),
            )
            .expect("mint node ext");
            node = builder.add(kind, span(), state.clone(), children, ext, ()).unwrap();
        }
        builder.finish(node).unwrap()
    }

    #[test]
    fn a_depth_limit_refuses_the_walk_of_a_deeper_tree() {
        // `a{b{c}}d` walks four levels deep (root list → group → group → chars).
        let tree = parse("a{b{c}}d");
        let mut log = Log::default();
        let error = TreeWalker::new(&mut log)
            .with_descent_guard_init(StdDescentGuardInit::depth_limit(2))
            .walk(tree.root())
            .unwrap_err();
        let WalkError::DescentLimitExceeded { detail } = error;
        assert!(detail.contains("depth_limit"), "{detail}");

        // A limit the tree fits under walks it whole.
        let mut log = Log::default();
        TreeWalker::new(&mut log)
            .with_descent_guard_init(StdDescentGuardInit::depth_limit(4))
            .walk(tree.root())
            .unwrap();
        assert_eq!(log.events.len(), 2 * 7, "all seven nodes entered and exited");
    }

    #[test]
    fn the_unconfigured_default_warns_the_visitor_once_then_refuses() {
        /// Counts descents and warnings; never stops on its own.
        #[derive(Default)]
        struct Meter {
            entered: usize,
            warnings: usize,
        }
        impl NodeVisitor<Latexlike, ()> for Meter {
            fn enter(
                &mut self,
                _node: NodeRef<'_, Latexlike>,
                _cx: &VisitContext<'_, Latexlike>,
            ) -> VisitFlow {
                self.entered += 1;
                VisitFlow::Descend
            }

            fn observe_descent_warning(&mut self, warning: DescentWarning) {
                self.warnings += 1;
                assert!(warning.detail.contains("with_descent_guard_init"));
            }
        }

        // Deep enough that even tiny walk frames exhaust the default budget.
        let tree = deep_group_chain(200_000);
        let mut meter = Meter::default();
        let error = TreeWalker::new(&mut meter).walk(tree.root()).unwrap_err();
        let WalkError::DescentLimitExceeded { detail } = error;
        assert!(detail.contains("DEFAULT_STACK_BUDGET"), "{detail}");
        assert_eq!(meter.warnings, 1, "the half-budget warning latches once");
        assert!(meter.entered > 0, "the walk made progress before refusing");

        // With the guard off, the same visitor walks a modest chain untroubled.
        let tree = deep_group_chain(16);
        let mut meter = Meter::default();
        TreeWalker::new(&mut meter)
            .with_descent_guard_init(StdDescentGuardInit::off())
            .walk(tree.root())
            .unwrap();
        assert_eq!((meter.entered, meter.warnings), (17, 0));
    }
}
