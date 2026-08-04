//! Tests of the recompose machinery (`techy::recompose`).

use core::convert::Infallible;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::engine::Language;
use crate::error::Recovery;
use crate::latexlike::{
    BodyMarker, CallableType, InvocationSyntaxData, Latexlike, LatexlikeDriver, MacroSpec,
};
use crate::node::{
    CallableData, ChildRegion, ContentNodes, NodeKind, NodeRef, NodeTree, NodeTreeBuilder,
    ParsedArguments, ParsedSlot, ParsedSlots, SlotRole,
};
use crate::scopes::Package;
use crate::source::{Source, SourceSpan, TextContent};
use crate::spec::StdCallableSpec;
use crate::state::{Lang, ParsingState};

use super::{
    core_source_instruction, recompose, ConcatPieces, Recompose, RecomposeContext,
    RecomposeError, Recomposer,
};

/// Parse `input` with the plain latexlike preset (strict).
fn parse(input: &str) -> NodeTree<Latexlike> {
    let language: Language<Latexlike> = Language::new(
        LatexlikeDriver::new(Recovery::Strict),
        ParsingState::lang_initial(),
    );
    let result = language.parse(input).expect("test inputs parse cleanly");
    assert!(result.diagnostics.is_empty(), "unexpected: {:?}", result.diagnostics);
    result.tree
}

/// The module-docs recomposer: core-complete kinds via
/// [`core_source_instruction`], callables are an error.
struct CoreEmitter;

impl<A> Recomposer<Latexlike, A> for CoreEmitter {
    type State = ();
    type Piece = String;
    type Error = String;

    fn recompose_node(
        &mut self,
        node: NodeRef<'_, Latexlike, A>,
        _state: &(),
        _cx: &mut RecomposeContext<'_, Latexlike, A>,
    ) -> Result<Recompose<String, ()>, String> {
        core_source_instruction(node).ok_or_else(|| "unexpected callable".to_string())
    }
}

// --- the value fold ---------------------------------------------------------------------

#[test]
fn the_fold_reemits_core_complete_trees() {
    // Chars + group (recorded delimiters as wrap head/tail) + root list.
    let tree = parse("a{b}c");
    assert_eq!(recompose(&tree, (), &mut CoreEmitter).unwrap(), "a{b}c");

    // Comments emit start + content + post-space from their own payload.
    let tree = parse("a% note\n  b");
    assert_eq!(recompose(&tree, (), &mut CoreEmitter).unwrap(), "a% note\n  b");
}

#[test]
fn wrap_and_join_compose_the_joiner_shape() {
    /// Root override: bracket the children and comma-join them.
    struct Joiner;
    impl<A> Recomposer<Latexlike, A> for Joiner {
        type State = ();
        type Piece = String;
        type Error = String;

        fn recompose_node(
            &mut self,
            node: NodeRef<'_, Latexlike, A>,
            _state: &(),
            _cx: &mut RecomposeContext<'_, Latexlike, A>,
        ) -> Result<Recompose<String, ()>, String> {
            if node.is_list() {
                Ok(Recompose::Concat(ConcatPieces::children().wrap("<", ">").join(", ")))
            } else {
                core_source_instruction(node).ok_or_else(|| "callable".to_string())
            }
        }
    }

    // head + child₁ + sep + child₂ + sep + child₃ + tail.
    let tree = parse("a{b}c");
    assert_eq!(recompose(&tree, (), &mut Joiner).unwrap(), "<a, {b}, c>");

    // A single child composes no separator…
    let tree = parse("x");
    assert_eq!(recompose(&tree, (), &mut Joiner).unwrap(), "<x>");

    // …and no children compose head + tail only.
    let tree = parse("");
    assert_eq!(recompose(&tree, (), &mut Joiner).unwrap(), "<>");
}

// --- state threading --------------------------------------------------------------------

/// Chars nodes emit `text@state`; containers bracket their children, deriving
/// `state + 1` when `derive` is set and inheriting otherwise.
struct Depther {
    derive: bool,
}

impl<A> Recomposer<Latexlike, A> for Depther {
    type State = usize;
    type Piece = String;
    type Error = Infallible;

    fn recompose_node(
        &mut self,
        node: NodeRef<'_, Latexlike, A>,
        state: &usize,
        _cx: &mut RecomposeContext<'_, Latexlike, A>,
    ) -> Result<Recompose<String, usize>, Infallible> {
        Ok(if let Some(text) = node.chars() {
            Recompose::Emit(format!("{text}@{state}"))
        } else {
            let mut pieces = ConcatPieces::children();
            if node.is_group() {
                pieces = pieces.wrap("[", "]");
            }
            if self.derive {
                pieces = pieces.with_state(state + 1);
            }
            Recompose::Concat(pieces)
        })
    }
}

#[test]
fn derived_state_threads_downward() {
    let tree = parse("a{b{c}}");
    let out = recompose(&tree, 0, &mut Depther { derive: true }).unwrap();
    assert_eq!(out, "a@1[b@2[c@3]]");
}

#[test]
fn without_a_derived_state_children_inherit_the_parents() {
    let tree = parse("a{b{c}}");
    let out = recompose(&tree, 7, &mut Depther { derive: false }).unwrap();
    assert_eq!(out, "a@7[b@7[c@7]]");
}

// --- the Concat scope -------------------------------------------------------------------

/// A hand-staged callable with Content, Attached, and Hidden slots (the
/// transform suite's fixture shape).
fn three_slot_fixture() -> NodeTree<Latexlike> {
    let source = Arc::new(Source::new("stub"));
    let span = || SourceSpan::new(&source, 0..4);
    let state = Arc::new(ParsingState::<Latexlike>::lang_initial());

    let mut builder: NodeTreeBuilder<Latexlike> = NodeTreeBuilder::new();
    let chars = |builder: &mut NodeTreeBuilder<Latexlike>, text: &str| {
        let kind = NodeKind::chars(TextContent::Owned(text.into()));
        let ext = <Latexlike as Lang>::make_node_ext(
            &kind,
            &span(),
            &state,
            builder.staged_children(&[]),
        );
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

/// Concat over the fixture's callable with the given widening choices; chars
/// children emit their text.
fn fold_with_scope(attached: bool, hidden: bool) -> String {
    struct Scoped {
        attached: bool,
        hidden: bool,
    }
    impl<A> Recomposer<Latexlike, A> for Scoped {
        type State = ();
        type Piece = String;
        type Error = Infallible;

        fn recompose_node(
            &mut self,
            node: NodeRef<'_, Latexlike, A>,
            _state: &(),
            _cx: &mut RecomposeContext<'_, Latexlike, A>,
        ) -> Result<Recompose<String, ()>, Infallible> {
            Ok(if let Some(text) = node.chars() {
                Recompose::Emit(text.to_string())
            } else {
                let mut pieces = ConcatPieces::children().join("+");
                if self.attached {
                    pieces = pieces.include_attached();
                }
                if self.hidden {
                    pieces = pieces.include_hidden();
                }
                Recompose::Concat(pieces)
            })
        }
    }
    let tree = three_slot_fixture();
    recompose(&tree, (), &mut Scoped { attached, hidden }).unwrap()
}

#[test]
fn the_default_concat_scope_skips_attached_and_hidden() {
    assert_eq!(fold_with_scope(false, false), "content");
}

#[test]
fn the_scope_widens_per_explicit_opt_in() {
    assert_eq!(fold_with_scope(true, false), "content+attached");
    assert_eq!(fold_with_scope(false, true), "content+hidden");
    assert_eq!(fold_with_scope(true, true), "content+attached+hidden");
}

// --- core_source_instruction ------------------------------------------------------------

#[test]
fn core_source_instruction_declines_callables() {
    let mut package = Package::new("t");
    package.insert(CallableType::Macro, "emph", MacroSpec::new(vec![]));
    let language: Language<Latexlike> = Language::new(
        LatexlikeDriver::new(Recovery::Strict),
        ParsingState::lang_initial_with_packages([package]),
    );
    let tree = language.parse("\\emph x").unwrap().tree;

    let emph = tree.root().child(0).unwrap();
    assert!(emph.is_callable());
    assert!(core_source_instruction::<_, _, String, ()>(emph).is_none());

    // The chars sibling answers. (It is "x", not " x": the whitespace after
    // the command word is the macro's recorded post_space payload, not
    // sibling content.)
    let chars = tree.root().child(1).unwrap();
    match core_source_instruction::<_, _, String, ()>(chars) {
        Some(Recompose::Emit(text)) => assert_eq!(text, "x"),
        other => panic!("expected an Emit instruction, got {other:?}"),
    }
}

// --- errors -----------------------------------------------------------------------------

#[test]
fn recomposer_errors_ride_through_typed() {
    struct Failer;
    impl<A> Recomposer<Latexlike, A> for Failer {
        type State = ();
        type Piece = String;
        type Error = String;

        fn recompose_node(
            &mut self,
            node: NodeRef<'_, Latexlike, A>,
            _state: &(),
            _cx: &mut RecomposeContext<'_, Latexlike, A>,
        ) -> Result<Recompose<String, ()>, String> {
            if node.chars() == Some("b") {
                return Err("boom".to_string());
            }
            core_source_instruction(node).ok_or_else(|| "callable".to_string())
        }
    }

    let tree = parse("a{b}c");
    let error = recompose(&tree, (), &mut Failer).unwrap_err();
    assert_eq!(error, RecomposeError::Recomposer("boom".to_string()));
    assert_eq!(error.to_string(), "recomposer failed: boom");
}

// --- unit pieces ------------------------------------------------------------------------

#[test]
fn unit_pieces_compose_for_free() {
    // Piece = (): the fold composes nothing; the recomposer accumulates in
    // its own fields (the walk-with-scoped-state shape).
    struct Counter {
        nodes: usize,
    }
    impl<A> Recomposer<Latexlike, A> for Counter {
        type State = usize; // nesting depth, threaded downward
        type Piece = ();
        type Error = Infallible;

        fn recompose_node(
            &mut self,
            _node: NodeRef<'_, Latexlike, A>,
            state: &usize,
            _cx: &mut RecomposeContext<'_, Latexlike, A>,
        ) -> Result<Recompose<(), usize>, Infallible> {
            self.nodes += 1;
            Ok(Recompose::Concat(ConcatPieces::children().with_state(state + 1)))
        }
    }

    let tree = parse("a{b{c}}d");
    let mut counter = Counter { nodes: 0 };
    recompose(&tree, 0, &mut counter).unwrap();
    assert_eq!(counter.nodes, tree.node_count());
}

// --- instruction constructors -----------------------------------------------------------

#[test]
fn concat_pieces_constructors_chain() {
    // The chainable spelling from the records; behavior is pinned by the
    // fold tests above — this pins the types (String pieces from &str, any
    // state type).
    let _: ConcatPieces<String, u32> =
        ConcatPieces::children().wrap("{", "}").join(", ").with_state(3);
    let _: ConcatPieces<(), ()> = ConcatPieces::children().include_attached().include_hidden();
}
