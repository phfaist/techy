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

use super::{restage, Restage, RestageContext, RestageError};

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

#[test]
fn descend_visits_slot_children_of_every_role() {
    // A callable with Content, Attached, and Hidden slots (framework-style,
    // hand-staged): the driver must visit children of all three uniformly.
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
    let input = builder.finish(root).unwrap();

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
