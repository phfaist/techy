//! Tests of the restage driver (`techy::transform`).

use core::convert::Infallible;

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::engine::Language;
use crate::error::Recovery;
use crate::latexlike::{
    argument_specs, CallableType, Latexlike, LatexlikeDriver, MacroSpec,
};
use crate::node::{
    validate_tree, NodeId, NodeKind, NodeRef, NodeTree, SlotRole,
};
use crate::scopes::Package;
use crate::source::TextContent;
use crate::state::{Lang, ParsingState};

use super::{
    restage, Restage, RestageContext, RestageError, RestageVisitor, RestagedArgument,
};

/// The documented reentrant-visitor error pattern: one boxed self-referential
/// `From` impl makes every context op `?`-propagatable.
#[derive(Clone, Debug, PartialEq, Eq)]
struct OpError(Box<RestageError<OpError>>);

impl From<RestageError<OpError>> for OpError {
    fn from(error: RestageError<OpError>) -> OpError {
        OpError(Box::new(error))
    }
}

impl core::fmt::Display for OpError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "op failed: {}", self.0)
    }
}

/// Parse `input` with the plain latexlike preset (strict).
fn parse(input: &str) -> NodeTree<Latexlike> {
    let language: Language<Latexlike> = Language::new(
        LatexlikeDriver::new(Recovery::Strict),
        ParsingState::lang_initial(),
    );
    let result = language.parse(input).expect("test inputs parse cleanly");
    assert!(result.diagnostics.is_empty(), "unexpected diagnostics: {:?}", result.diagnostics);
    result.tree
}

/// Parse `input` with macros `\a{…}{…}` (two mandatory brace arguments, the
/// second named "closing") and `\o[…]{…}` (optional + mandatory).
fn parse_with_macros(input: &str) -> NodeTree<Latexlike> {
    let mut package = Package::new("test-macros");
    let mut a_args = argument_specs(&["{", "{"]).unwrap();
    let second = Arc::get_mut(&mut a_args[1]).expect("fresh spec");
    second.name = Some("closing".into());
    package.insert(CallableType::Macro, "a", MacroSpec::new(a_args));
    package.insert(
        CallableType::Macro,
        "o",
        MacroSpec::new(argument_specs(&["[", "{"]).unwrap()),
    );
    let language: Language<Latexlike> = Language::new(
        LatexlikeDriver::new(Recovery::Strict),
        ParsingState::lang_initial_with_packages([package]),
    );
    let result = language.parse(input).expect("test inputs parse cleanly");
    assert!(result.diagnostics.is_empty(), "unexpected diagnostics: {:?}", result.diagnostics);
    result.tree
}

/// The origin-tracking convention's annotation type.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Origin {
    original: NodeId,
}

/// A `Descend`-everything closure visitor annotating each copy with its
/// original node (the module-docs recipe).
fn origins(
    node: NodeRef<'_, Latexlike>,
    _cx: &mut RestageContext<'_, Latexlike, (), Origin>,
) -> Result<Restage<Origin>, Infallible> {
    Ok(Restage::Descend(Origin { original: node.id() }))
}

// --- the identity pass: annotation flow -------------------------------------------------

#[test]
fn identity_restage_preserves_structure_and_mints_annotations() {
    let input = parse("a{b}c");
    let output: NodeTree<Latexlike, Origin> = restage(&input, &mut origins).unwrap();

    validate_tree(&output).unwrap();
    assert_eq!(output.node_count(), input.node_count());
    // Identity restaging preserves the breadth-first layout: nodes correspond
    // 1:1 in storage order, and every annotation names its original.
    for (old, new) in input.iter_storage_order().zip(output.iter_storage_order()) {
        assert_eq!(new.annotation().original, old.id());
        assert_eq!(new.kind().as_str(), old.kind().as_str());
        assert_eq!(new.span(), old.span());
        assert_eq!(new.child_count(), old.child_count());
    }
    // The trees are distinct layouts: ids are tagged apart.
    assert_ne!(output.root().id(), input.root().id());
}

#[test]
fn the_closure_blanket_supports_inline_closures() {
    // The `restage(&tree, &mut |node, cx| …)` spelling from the records — the
    // closure needs its parameter types spelled once (two generic parameters
    // under a higher-ranked bound), everything else infers.
    let input = parse("hello");
    let output = restage(
        &input,
        &mut |node: NodeRef<'_, Latexlike>,
              _cx: &mut RestageContext<'_, Latexlike, (), u32>| {
            Ok::<_, Infallible>(Restage::Descend(node.span().len() as u32))
        },
    )
    .unwrap();
    assert_eq!(*output.root().annotation(), 5);
}

// --- Emit: drop, replace, no-descent ----------------------------------------------------

#[test]
fn emit_empty_drops_the_subtree() {
    let input = parse("a{b}c");
    let output = restage(
        &input,
        &mut |node: NodeRef<'_, Latexlike>,
              _cx: &mut RestageContext<'_, Latexlike, (), ()>| {
            Ok::<_, Infallible>(if node.is_group() {
                Restage::Emit(vec![])
            } else {
                Restage::Descend(())
            })
        },
    )
    .unwrap();

    validate_tree(&output).unwrap();
    let root = output.root();
    assert_eq!(root.child_count(), 2);
    assert_eq!(root.child(0).unwrap().chars(), Some("a"));
    assert_eq!(root.child(1).unwrap().chars(), Some("c"));
}

#[test]
fn emit_replaces_without_descending() {
    // Replace the whole group with a synthesized chars node (the explicit
    // make_node_ext staging recipe on the raw builder) and count visits: the
    // group's children must never be visited.
    let input = parse("x{y}z");
    let mut visited: Vec<String> = Vec::new();
    let output = restage(
        &input,
        &mut |node: NodeRef<'_, Latexlike>,
              cx: &mut RestageContext<'_, Latexlike, (), ()>| {
            visited.push(node.summary());
            if node.is_group() {
                let kind: NodeKind<Latexlike> =
                    NodeKind::chars(TextContent::Owned("Q".into()));
                let span = node.span().clone();
                let state = node.parsing_state().clone();
                let builder = cx.builder();
                let ext = <Latexlike as Lang>::make_node_ext(
                    &kind,
                    &span,
                    &state,
                    builder.staged_children(&[]),
                );
                let id = builder
                    .add(kind, span, state, Vec::new(), ext, ())
                    .map_err(RestageError::<Infallible>::Build)?;
                Ok(Restage::Emit(vec![id]))
            } else {
                Ok::<_, RestageError<Infallible>>(Restage::Descend(()))
            }
        },
    )
    .unwrap();

    validate_tree(&output).unwrap();
    assert_eq!(output.root().child(1).unwrap().chars(), Some("Q"));
    assert_eq!(output.root().child_count(), 3);
    // Root, "x", the group, "z" — but never "y".
    assert!(visited.iter().all(|s| s != "chars(y)"), "visited: {visited:?}");
    assert_eq!(visited.len(), 4);
}

// --- error paths ------------------------------------------------------------------------

#[test]
fn dropping_the_root_is_root_not_singular() {
    let input = parse("a");
    let result = restage(
        &input,
        &mut |_node: NodeRef<'_, Latexlike>,
              _cx: &mut RestageContext<'_, Latexlike, (), ()>| {
            Ok::<_, Infallible>(Restage::Emit(vec![]))
        },
    );
    assert!(matches!(result, Err(RestageError::RootNotSingular { count: 0 })));
}

#[test]
fn visitor_errors_ride_through_typed() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct BadNode(NodeId);

    impl core::fmt::Display for BadNode {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "bad node {:?}", self.0)
        }
    }

    let input = parse("a{b}c");
    let group_id = input.root().child(1).unwrap().id();
    let result: Result<NodeTree<Latexlike, ()>, _> = restage(
        &input,
        &mut |node: NodeRef<'_, Latexlike>,
              _cx: &mut RestageContext<'_, Latexlike, (), ()>| {
            if node.is_group() {
                Err(BadNode(node.id()))
            } else {
                Ok(Restage::Descend(()))
            }
        },
    );
    let error = result.unwrap_err();
    assert_eq!(error, RestageError::Visitor(BadNode(group_id)));
    // The uniform-Clone principle, conditionally: E: Clone ⇒ the error clones.
    let _ = error.clone();
    assert!(error.to_string().contains("restage visitor failed"));
}

#[test]
fn dropped_content_parent_is_diagnosed_with_the_takeover_route() {
    // \a{1}{2}: each argument's content is designated inside its group node
    // (`InChildrenOf`). Dropping a group while Descend-ing the callable leaves
    // the record unanchorable — the driver refuses with the diagnosis.
    let input = parse_with_macros(r"\a{1}{2}");
    let dropped_group = input.root().child(0).unwrap().child(0).unwrap();
    assert!(dropped_group.is_group());

    let result = restage(
        &input,
        &mut |node: NodeRef<'_, Latexlike>,
              _cx: &mut RestageContext<'_, Latexlike, (), ()>| {
            Ok::<_, Infallible>(if node.id() == dropped_group.id() {
                Restage::Emit(vec![])
            } else {
                Restage::Descend(())
            })
        },
    );
    let error = result.unwrap_err();
    let RestageError::ContentParentDropped { callable, parent, replaced_by } = &error else {
        panic!("expected ContentParentDropped, got {error:?}");
    };
    assert_eq!(*callable, input.root().child(0).unwrap().id());
    assert_eq!(*parent, dropped_group.id());
    assert_eq!(*replaced_by, Some(0));
    let message = error.to_string();
    assert!(message.contains("take over the callable"), "message: {message}");
    assert!(message.contains("restage_invocation"), "message: {message}");
}

#[test]
fn emptied_region_restages_as_provided_with_empty_region() {
    // Dropping the *content* of an argument (not its group wrapper) empties the
    // region's content — the argument stays provided (absent ≠ empty).
    let input = parse_with_macros(r"\a{1}{2}");
    let output = restage(
        &input,
        &mut |node: NodeRef<'_, Latexlike>,
              _cx: &mut RestageContext<'_, Latexlike, (), ()>| {
            Ok::<_, Infallible>(if node.chars() == Some("1") {
                Restage::Emit(vec![])
            } else {
                Restage::Descend(())
            })
        },
    )
    .unwrap();

    validate_tree(&output).unwrap();
    let callable = output.root().child(0).unwrap();
    let arguments = callable.arguments().unwrap();
    assert!(arguments.get(0).unwrap().is_provided());
    let content = callable.argument_content_nodes(0).unwrap();
    assert!(content.is_empty());
    assert_eq!(
        callable
            .argument_content_nodes(1)
            .unwrap()
            .first()
            .unwrap()
            .chars(),
        Some("2")
    );
}

// --- structural descent: slot roles -----------------------------------------------------

/// A hand-staged callable with Content, Attached, and Hidden slots
/// (framework-style) — the fixture of the descent and slot-op tests.
fn three_slot_fixture() -> NodeTree<Latexlike> {
    use crate::latexlike::{BodyMarker, InvocationSyntaxData};
    use crate::node::{
        CallableData, ChildRegion, ContentNodes, NodeTreeBuilder, ParsedArguments,
        ParsedSlot, ParsedSlots,
    };
    use crate::source::{Source, SourceSpan};
    use crate::spec::StdCallableSpec;

    let source = Arc::new(Source::new("stub"));
    let span = || SourceSpan::new(&source, 0..4);
    let state = Arc::new(ParsingState::<Latexlike>::lang_initial());

    let mut builder: NodeTreeBuilder<Latexlike> = NodeTreeBuilder::new();
    let chars = |builder: &mut NodeTreeBuilder<Latexlike>, text: &str| {
        let kind = NodeKind::chars(TextContent::Owned(text.into()));
        let ext =
            <Latexlike as Lang>::make_node_ext(&kind, &span(), &state, builder.staged_children(&[]));
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
    );
    let root = builder.add(kind, span(), state.clone(), children, ext, ()).unwrap();
    builder.finish(root).unwrap()
}

#[test]
fn descend_visits_slot_children_of_every_role() {
    // The driver must visit slot children of all three roles uniformly.
    let input = three_slot_fixture();

    let mut seen: Vec<String> = Vec::new();
    let output = restage(
        &input,
        &mut |node: NodeRef<'_, Latexlike>,
              _cx: &mut RestageContext<'_, Latexlike, (), ()>| {
            if let Some(text) = node.chars() {
                seen.push(text.to_string());
            }
            Ok::<_, Infallible>(Restage::Descend(()))
        },
    )
    .unwrap();

    assert_eq!(seen, ["content", "attached", "hidden"]);
    // The roles survive the restage verbatim.
    let slots = output.root().slots().unwrap();
    let roles: Vec<SlotRole> = slots.iter().map(|slot| slot.role).collect();
    assert_eq!(roles, [SlotRole::Content, SlotRole::Attached, SlotRole::Hidden]);
}

// --- region ops + bundles (M3) ----------------------------------------------------------

/// The T5 paradigm: swap the two arguments of `\a` — two `restage_argument`
/// calls plus one reordered `restage_invocation` (a reentrant trait visitor:
/// the ops re-enter `self` for the region nodes).
struct SwapArguments;

impl RestageVisitor<Latexlike, (), ()> for SwapArguments {
    type Error = OpError;

    fn restage(
        &mut self,
        node: NodeRef<'_, Latexlike>,
        cx: &mut RestageContext<'_, Latexlike, (), ()>,
    ) -> Result<Restage<()>, OpError> {
        if node.name() == Some("a") {
            let first = cx.restage_argument(node, 0, self)?;
            let second = cx.restage_argument_named(node, "closing", self)?;
            let id = cx.restage_invocation(node, vec![second, first], vec![], ())?;
            Ok(Restage::Emit(vec![id]))
        } else {
            Ok(Restage::Descend(()))
        }
    }
}

#[test]
fn argument_swap_round_trip() {
    let input = parse_with_macros(r"\a{1}{2}");
    let output = restage(&input, &mut SwapArguments).unwrap();

    validate_tree(&output).unwrap();
    let callable = output.root().child(0).unwrap();
    assert_eq!(callable.name(), Some("a"));

    // The records swapped wholesale — content, spans, and specs travel together.
    let arguments = callable.arguments().unwrap();
    assert_eq!(arguments.len(), 2);
    assert_eq!(arguments.get(0).unwrap().name(), Some("closing"));
    assert_eq!(arguments.get(1).unwrap().name(), None);
    let first_content = callable.argument_content_nodes(0).unwrap();
    assert_eq!(first_content.first().unwrap().chars(), Some("2"));
    let second_content = callable.argument_content_nodes(1).unwrap();
    assert_eq!(second_content.first().unwrap().chars(), Some("1"));

    // Sibling spans are now out of source order — legal in transform trees.
    let group_spans: Vec<_> = callable
        .children()
        .iter()
        .map(|child| child.span().range())
        .collect();
    assert_eq!(group_spans, [5..8, 2..5]);
    // The callable's own identity is carried verbatim.
    assert_eq!(callable.span().range(), input.root().child(0).unwrap().span().range());
}

#[test]
fn absent_arguments_transfer_presence() {
    // \o's optional argument is not provided: the bundle is absent, and the
    // restaged record stays absent (absent ≠ empty).
    struct Reinvoke;
    impl RestageVisitor<Latexlike, (), ()> for Reinvoke {
        type Error = OpError;
        fn restage(
            &mut self,
            node: NodeRef<'_, Latexlike>,
            cx: &mut RestageContext<'_, Latexlike, (), ()>,
        ) -> Result<Restage<()>, OpError> {
            if node.name() == Some("o") {
                let optional = cx.restage_argument(node, 0, self)?;
                assert!(!optional.is_provided());
                assert!(optional.nodes().is_empty());
                let mandatory = cx.restage_argument(node, 1, self)?;
                assert!(mandatory.is_provided());
                let id = cx.restage_invocation(node, vec![optional, mandatory], vec![], ())?;
                Ok(Restage::Emit(vec![id]))
            } else {
                Ok(Restage::Descend(()))
            }
        }
    }

    let input = parse_with_macros(r"\o{x}");
    let output = restage(&input, &mut Reinvoke).unwrap();
    validate_tree(&output).unwrap();
    let callable = output.root().child(0).unwrap();
    let arguments = callable.arguments().unwrap();
    assert!(!arguments.get(0).unwrap().is_provided());
    assert!(arguments.get(1).unwrap().is_provided());
    assert_eq!(
        callable.argument_content_nodes(1).unwrap().first().unwrap().chars(),
        Some("x")
    );
}

#[test]
fn unwrapping_groups_via_restage_children() {
    struct UnwrapGroups;
    impl RestageVisitor<Latexlike, (), ()> for UnwrapGroups {
        type Error = OpError;
        fn restage(
            &mut self,
            node: NodeRef<'_, Latexlike>,
            cx: &mut RestageContext<'_, Latexlike, (), ()>,
        ) -> Result<Restage<()>, OpError> {
            if node.is_group() {
                Ok(Restage::Emit(cx.restage_children(node, self)?))
            } else {
                Ok(Restage::Descend(()))
            }
        }
    }

    let input = parse("a{b{c}}d");
    let output = restage(&input, &mut UnwrapGroups).unwrap();
    validate_tree(&output).unwrap();
    let texts: Vec<_> = output
        .root()
        .children()
        .iter()
        .map(|child| child.chars().unwrap().to_string())
        .collect();
    assert_eq!(texts, ["a", "b", "c", "d"]);
}

#[test]
fn restage_subtree_duplicates_and_redrives() {
    // Driving the same node twice is legal: replace "x" with a fresh restage
    // of the group's subtree, while the group is also driven as itself.
    struct Duplicate {
        group: NodeId,
    }
    impl RestageVisitor<Latexlike, (), ()> for Duplicate {
        type Error = OpError;
        fn restage(
            &mut self,
            node: NodeRef<'_, Latexlike>,
            cx: &mut RestageContext<'_, Latexlike, (), ()>,
        ) -> Result<Restage<()>, OpError> {
            if node.chars() == Some("x") {
                let group = node.tree().node(self.group);
                Ok(Restage::Emit(cx.restage_subtree(group, self)?))
            } else {
                Ok(Restage::Descend(()))
            }
        }
    }

    let input = parse("x{y}");
    let group = input.root().child(1).unwrap().id();
    let output = restage(&input, &mut Duplicate { group }).unwrap();
    validate_tree(&output).unwrap();
    let root = output.root();
    assert_eq!(root.child_count(), 2);
    assert!(root.child(0).unwrap().is_group());
    assert!(root.child(1).unwrap().is_group());
    assert_eq!(root.child(0).unwrap().child(0).unwrap().chars(), Some("y"));
    assert_eq!(root.child(1).unwrap().child(0).unwrap().chars(), Some("y"));
    assert_ne!(root.child(0).unwrap().id(), root.child(1).unwrap().id());
}

#[test]
fn restage_slot_carries_name_role_and_content() {
    // Restage slot 1 (Attached) of the fixture and re-invoke with just it.
    struct KeepAttached;
    impl RestageVisitor<Latexlike, (), ()> for KeepAttached {
        type Error = OpError;
        fn restage(
            &mut self,
            node: NodeRef<'_, Latexlike>,
            cx: &mut RestageContext<'_, Latexlike, (), ()>,
        ) -> Result<Restage<()>, OpError> {
            if node.is_callable() {
                let slot = cx.restage_slot(node, 1, self)?;
                assert_eq!(slot.role(), SlotRole::Attached);
                assert_eq!(slot.name(), None);
                let id = cx.restage_invocation(node, vec![], vec![slot], ())?;
                Ok(Restage::Emit(vec![id]))
            } else {
                Ok(Restage::Descend(()))
            }
        }
    }

    let input = three_slot_fixture();
    let output = restage(&input, &mut KeepAttached).unwrap();
    validate_tree(&output).unwrap();
    let callable = output.root();
    assert!(callable.arguments().unwrap().is_empty());
    let slots = callable.slots().unwrap();
    assert_eq!(slots.len(), 1);
    assert_eq!(slots.get(0).unwrap().role, SlotRole::Attached);
    assert_eq!(callable.child_count(), 1);
    assert_eq!(
        callable.slot_content_nodes(0).unwrap().first().unwrap().chars(),
        Some("attached")
    );
}

#[test]
fn op_misuse_is_diagnosed_not_panicked() {
    struct Misuse;
    impl RestageVisitor<Latexlike, (), ()> for Misuse {
        type Error = OpError;
        fn restage(
            &mut self,
            node: NodeRef<'_, Latexlike>,
            cx: &mut RestageContext<'_, Latexlike, (), ()>,
        ) -> Result<Restage<()>, OpError> {
            if node.name() == Some("a") {
                // Unknown argument name:
                let error = cx.restage_argument_named(node, "nonsense", self).unwrap_err();
                assert!(matches!(
                    &error,
                    RestageError::UnknownArgumentName { name, .. } if name == "nonsense"
                ));
                // Argument index out of range:
                let error = cx.restage_argument(node, 7, self).unwrap_err();
                assert!(matches!(
                    error,
                    RestageError::ArgumentIndexOutOfRange { index: 7, count: 2, .. }
                ));
                // Slot index out of range (macros have no slots):
                let error = cx.restage_slot(node, 0, self).unwrap_err();
                assert!(matches!(
                    error,
                    RestageError::SlotIndexOutOfRange { index: 0, count: 0, .. }
                ));
            } else if node.is_chars() {
                // Region ops demand a callable:
                let error = cx.restage_argument(node, 0, self).unwrap_err();
                assert!(matches!(error, RestageError::NotACallable { .. }));
                let error: RestageError<OpError> =
                    cx.restage_invocation(node, vec![], vec![], ()).unwrap_err();
                assert!(matches!(error, RestageError::NotACallable { .. }));
            }
            Ok(Restage::Descend(()))
        }
    }

    let input = parse_with_macros(r"z\a{1}{2}");
    let output = restage(&input, &mut Misuse).unwrap();
    validate_tree(&output).unwrap();
}

#[test]
fn hand_built_bundles_are_first_class() {
    // The constructors are the general take-both form: build an argument bundle
    // from scratch (fresh content, InRegion designation) and re-invoke with it.
    use crate::node::ContentNodes;

    struct Rewrite;
    impl RestageVisitor<Latexlike, (), ()> for Rewrite {
        type Error = OpError;
        fn restage(
            &mut self,
            node: NodeRef<'_, Latexlike>,
            cx: &mut RestageContext<'_, Latexlike, (), ()>,
        ) -> Result<Restage<()>, OpError> {
            if node.name() != Some("a") {
                return Ok(Restage::Descend(()));
            }
            let spec = Arc::clone(&node.arguments().unwrap().get(0).unwrap().spec);
            let second = cx.restage_argument(node, 1, self)?;

            // A fresh single-node region whose one node is itself the content.
            let kind: NodeKind<Latexlike> = NodeKind::chars(TextContent::Owned("NEW".into()));
            let span = node.span().clone();
            let state = node.parsing_state().clone();
            let builder = cx.builder();
            let ext = <Latexlike as Lang>::make_node_ext(
                &kind,
                &span,
                &state,
                builder.staged_children(&[]),
            );
            let fresh = builder
                .add(kind, span, state, Vec::new(), ext, ())
                .map_err(RestageError::<OpError>::Build)?;
            let first = RestagedArgument::provided(
                spec,
                vec![fresh],
                ContentNodes::InRegion(0..1),
                (),
            );
            let id = cx.restage_invocation(node, vec![first, second], vec![], ())?;
            Ok(Restage::Emit(vec![id]))
        }
    }

    let input = parse_with_macros(r"\a{1}{2}");
    let output = restage(&input, &mut Rewrite).unwrap();
    validate_tree(&output).unwrap();
    let callable = output.root().child(0).unwrap();
    assert_eq!(
        callable.argument_content_nodes(0).unwrap().first().unwrap().chars(),
        Some("NEW")
    );
    assert_eq!(
        callable.argument_content_nodes(1).unwrap().first().unwrap().chars(),
        Some("2")
    );
}

// --- content-swap helpers (M4) ----------------------------------------------------------

/// Stage a fresh owned-content chars node through the raw builder (the
/// explicit `make_node_ext` recipe), annotated.
fn stage_chars<B>(
    cx: &mut RestageContext<'_, Latexlike, (), B>,
    at: NodeRef<'_, Latexlike>,
    text: &str,
    annotation: B,
) -> Result<crate::node::BuildId, RestageError<OpError>>
where
    B: Clone + core::fmt::Debug + Send + Sync,
{
    let kind: NodeKind<Latexlike> = NodeKind::chars(TextContent::Owned(text.into()));
    let span = at.span().clone();
    let state = at.parsing_state().clone();
    let builder = cx.builder();
    let ext =
        <Latexlike as Lang>::make_node_ext(&kind, &span, &state, builder.staged_children(&[]));
    builder.add(kind, span, state, Vec::new(), ext, annotation).map_err(RestageError::Build)
}

#[test]
fn content_swap_keeps_wrapper_and_noise_verbatim() {
    // `\a{1} {2}`: argument 1's region is [whitespace noise, group{2}] (the
    // whitespace scanned while looking for the argument is region noise) — the
    // swap keeps both verbatim and re-anchors the designation inside the
    // group's copy.
    struct Swap;
    impl RestageVisitor<Latexlike, (), ()> for Swap {
        type Error = OpError;
        fn restage(
            &mut self,
            node: NodeRef<'_, Latexlike>,
            cx: &mut RestageContext<'_, Latexlike, (), ()>,
        ) -> Result<Restage<()>, OpError> {
            if node.name() == Some("a") {
                let fresh = stage_chars(cx, node, "NEW", ())?;
                let first = cx.restage_argument(node, 0, self)?;
                let second = cx.restage_argument_with_content(node, 1, vec![fresh], ())?;
                let id = cx.restage_invocation(node, vec![first, second], vec![], ())?;
                Ok(Restage::Emit(vec![id]))
            } else {
                Ok(Restage::Descend(()))
            }
        }
    }

    let input = parse_with_macros("\\a{1} {2}");
    // Sanity: the input's second region really carries leading noise.
    assert_eq!(input.root().child(0).unwrap().argument_nodes(1).unwrap().len(), 2);

    let output = restage(&input, &mut Swap).unwrap();
    validate_tree(&output).unwrap();
    let callable = output.root().child(0).unwrap();

    // Region shape preserved: noise chars node, then the group wrapper.
    let region = callable.argument_nodes(1).unwrap();
    assert_eq!(region.len(), 2);
    assert_eq!(region.get(0).unwrap().chars(), Some(" "));
    let group = region.get(1).unwrap();
    assert!(group.is_group());
    assert_eq!(group.group_delimiters(), Some(("{", "}")));
    // Content swapped, designation re-anchored onto the group's copy.
    let content = callable.argument_content_nodes(1).unwrap();
    assert_eq!(content.len(), 1);
    assert_eq!(content.first().unwrap().chars(), Some("NEW"));
    assert!(callable.arguments().unwrap().get(1).unwrap().is_provided());
}

#[test]
fn content_swap_reanchors_through_deep_wrappers() {
    // `\o[{deep}]{x}`: the optional argument's lone inner group protects and
    // unwraps — its content parent sits at grandchild depth. The swap copies
    // the outer `[…]` and inner `{…}` wrappers verbatim and swaps inside.
    struct Swap;
    impl RestageVisitor<Latexlike, (), ()> for Swap {
        type Error = OpError;
        fn restage(
            &mut self,
            node: NodeRef<'_, Latexlike>,
            cx: &mut RestageContext<'_, Latexlike, (), ()>,
        ) -> Result<Restage<()>, OpError> {
            if node.name() == Some("o") {
                let fresh = stage_chars(cx, node, "SWAPPED", ())?;
                let optional = cx.restage_argument_with_content(node, 0, vec![fresh], ())?;
                let mandatory = cx.restage_argument(node, 1, self)?;
                let id = cx.restage_invocation(node, vec![optional, mandatory], vec![], ())?;
                Ok(Restage::Emit(vec![id]))
            } else {
                Ok(Restage::Descend(()))
            }
        }
    }

    let input = parse_with_macros(r"\o[{deep}]{x}");
    // Sanity: the input's designated content parent is the inner group.
    let input_callable = input.root().child(0).unwrap();
    let inner = input_callable.argument_content_nodes(0).unwrap().first().unwrap().parent().unwrap();
    assert!(inner.is_group());
    assert_ne!(inner.id(), input_callable.id());

    let output = restage(&input, &mut Swap).unwrap();
    validate_tree(&output).unwrap();
    let callable = output.root().child(0).unwrap();
    let region = callable.argument_nodes(0).unwrap();
    assert_eq!(region.len(), 1);
    let outer = region.first().unwrap();
    assert!(outer.is_group());
    assert_eq!(outer.group_delimiters(), Some(("[", "]")));
    let inner = outer.child(0).unwrap();
    assert!(inner.is_group());
    assert_eq!(inner.group_delimiters(), Some(("{", "}")));
    let content = callable.argument_content_nodes(0).unwrap();
    assert_eq!(content.len(), 1);
    assert_eq!(content.first().unwrap().chars(), Some("SWAPPED"));
}

#[test]
fn content_swap_fills_an_empty_argument() {
    // `\a{}{2}`: an empty-but-provided content range anchored inside the group
    // — the swap splices at the anchored position.
    struct Fill;
    impl RestageVisitor<Latexlike, (), ()> for Fill {
        type Error = OpError;
        fn restage(
            &mut self,
            node: NodeRef<'_, Latexlike>,
            cx: &mut RestageContext<'_, Latexlike, (), ()>,
        ) -> Result<Restage<()>, OpError> {
            if node.name() == Some("a") {
                let fresh = stage_chars(cx, node, "filled", ())?;
                let first = cx.restage_argument_with_content(node, 0, vec![fresh], ())?;
                let second = cx.restage_argument(node, 1, self)?;
                let id = cx.restage_invocation(node, vec![first, second], vec![], ())?;
                Ok(Restage::Emit(vec![id]))
            } else {
                Ok(Restage::Descend(()))
            }
        }
    }

    let input = parse_with_macros(r"\a{}{2}");
    assert!(input.root().child(0).unwrap().argument_content_nodes(0).unwrap().is_empty());
    let output = restage(&input, &mut Fill).unwrap();
    validate_tree(&output).unwrap();
    let callable = output.root().child(0).unwrap();
    assert_eq!(
        callable.argument_content_nodes(0).unwrap().first().unwrap().chars(),
        Some("filled")
    );
}

#[test]
fn content_swap_on_a_slot_and_its_misuse_errors() {
    struct SwapSlot;
    impl RestageVisitor<Latexlike, (), ()> for SwapSlot {
        type Error = OpError;
        fn restage(
            &mut self,
            node: NodeRef<'_, Latexlike>,
            cx: &mut RestageContext<'_, Latexlike, (), ()>,
        ) -> Result<Restage<()>, OpError> {
            if node.is_callable() {
                let fresh = stage_chars(cx, node, "swapped-slot", ())?;
                let slot = cx.restage_slot_with_content(node, 0, vec![fresh], ())?;
                assert_eq!(slot.role(), SlotRole::Content);
                let others = [
                    cx.restage_slot(node, 1, self)?,
                    cx.restage_slot(node, 2, self)?,
                ];
                let mut slots = vec![slot];
                slots.extend(others);
                let id = cx.restage_invocation(node, vec![], slots, ())?;
                Ok(Restage::Emit(vec![id]))
            } else {
                Ok(Restage::Descend(()))
            }
        }
    }

    let input = three_slot_fixture();
    let output = restage(&input, &mut SwapSlot).unwrap();
    validate_tree(&output).unwrap();
    let callable = output.root();
    assert_eq!(
        callable.slot_content_nodes(0).unwrap().first().unwrap().chars(),
        Some("swapped-slot")
    );
    assert_eq!(
        callable.slot_content_nodes(1).unwrap().first().unwrap().chars(),
        Some("attached")
    );

    // Misuse: swapping content of an absent argument is refused.
    struct AbsentSwap;
    impl RestageVisitor<Latexlike, (), ()> for AbsentSwap {
        type Error = OpError;
        fn restage(
            &mut self,
            node: NodeRef<'_, Latexlike>,
            cx: &mut RestageContext<'_, Latexlike, (), ()>,
        ) -> Result<Restage<()>, OpError> {
            if node.name() == Some("o") {
                let fresh = stage_chars(cx, node, "zzz", ())?;
                let error: RestageError<OpError> = cx
                    .restage_argument_with_content(node, 0, vec![fresh], ())
                    .unwrap_err();
                assert!(matches!(error, RestageError::ArgumentAbsent { index: 0, .. }));
            }
            Ok(Restage::Descend(()))
        }
    }
    // NOTE: the staged-but-unused `fresh` node is dropped by finish() (staged
    // nodes unreachable from the root are dropped — builder contract).
    let input = parse_with_macros(r"\o{x}");
    let output = restage(&input, &mut AbsentSwap).unwrap();
    validate_tree(&output).unwrap();
}

#[test]
fn content_swap_annotations_flow_explicitly() {
    // The helper's `annotation` argument lands (cloned) on every verbatim
    // wrapper/noise copy; the caller-staged content keeps its own annotations.
    struct Swap;
    impl RestageVisitor<Latexlike, (), u8> for Swap {
        type Error = OpError;
        fn restage(
            &mut self,
            node: NodeRef<'_, Latexlike>,
            cx: &mut RestageContext<'_, Latexlike, (), u8>,
        ) -> Result<Restage<u8>, OpError> {
            if node.name() == Some("a") {
                let fresh = stage_chars(cx, node, "NEW", 9)?;
                let first = cx.restage_argument(node, 0, self)?;
                let second = cx.restage_argument_with_content(node, 1, vec![fresh], 7)?;
                let id = cx.restage_invocation(node, vec![first, second], vec![], 5)?;
                Ok(Restage::Emit(vec![id]))
            } else {
                Ok(Restage::Descend(1))
            }
        }
    }

    let input = parse_with_macros("\\a{1} {2}");
    let output = restage(&input, &mut Swap).unwrap();
    validate_tree(&output).unwrap();
    let callable = output.root().child(0).unwrap();
    assert_eq!(*callable.annotation(), 5);
    let region = callable.argument_nodes(1).unwrap();
    assert_eq!(*region.get(0).unwrap().annotation(), 7); // noise copy
    assert_eq!(*region.get(1).unwrap().annotation(), 7); // group wrapper copy
    let content = callable.argument_content_nodes(1).unwrap();
    assert_eq!(*content.first().unwrap().annotation(), 9); // caller-staged
    // The visitor-driven first argument flowed through Descend(1).
    let first = callable.argument_content_nodes(0).unwrap();
    assert_eq!(*first.first().unwrap().annotation(), 1);
}
