//! The tree checker for latexlike parses (test builds only).
//!
//! [`check_latexlike_tree_invariants`] is what the crate's own tests assert on every
//! tree a latexlike parse produces. A passing check means two things hold: the core
//! span-tiling law ([`check_tree_invariants`]), and, on top of it, that each callable
//! node's recorded invocation syntax ([`InvocationSyntaxData`]) agrees byte for byte
//! with what the node's span actually spells.
//!
//! Core cannot make the second check itself. The invocation-syntax payload type
//! belongs to the language, so core has no way to read it; the checks for the
//! preset's own payload therefore live here, with the preset that defines it.
//!
//! This is an in-crate test oracle, not something a finished tree is required to
//! satisfy in general. The public check, and the one integration tests use, is
//! [`validate_tree`](crate::node::validate_tree), which never covered the payload.
// Mechanism mirror of the core checker: `pub(crate)` + `#[cfg(test)]`
// (cf. D-plan-12 Option B).

use alloc::string::String;

use crate::node::{
    check_tree_invariants, CallableData, NodeData, NodeKind, NodeTree,
};
use crate::source::TextContent;

use super::invocation_syntax::{
    EnvironmentSyntax, InvocationSyntaxData, StdEnvironmentSyntax,
};
use super::lang::LatexlikeLang;

/// Checks a finished latexlike-family tree, panicking with a description of the first
/// violation found.
///
/// A passing check means the tree satisfies the core span-tiling law
/// ([`check_tree_invariants`]: the all-trees structural law plus byte accounting)
/// *and* that every callable node's recorded invocation syntax matches the bytes its
/// span covers — see [`check_invocation_syntax_payload`] for the checks, per
/// invocation form.
///
/// Use it in a test on any tree a latexlike-family parse produced.
///
/// The payload checks are byte accounting, so they apply only to a language that
/// obeys span tiling
/// ([`Lang::OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING)). For a
/// language declaring `false` this is exactly [`check_tree_invariants`], which there
/// checks the all-trees law alone: a recorded spelling may be owned text, and the
/// node's span need not contain the trigger's bytes at all.
pub(crate) fn check_latexlike_tree_invariants<LLL: LatexlikeLang, A>(
    tree: &NodeTree<LLL, A>,
) {
    check_tree_invariants(tree);
    if !LLL::OBEYS_SPAN_TILING {
        return;
    }
    for (i, data) in tree.nodes().iter().enumerate() {
        if let NodeKind::Callable(callable) = &data.kind {
            check_invocation_syntax_payload(tree, i, data, callable);
        }
    }
}

/// Checks one callable node's recorded invocation syntax against the bytes its span
/// covers (the payload half of [`check_latexlike_tree_invariants`]).
///
/// Only the preset's own payload is checked: the [`InvocationSyntaxData`] enum over
/// the standard environment record, reached by `Any` downcast. A payload of any other
/// type — a custom `Env`, a record belonging to another language — follows that
/// language's own recording rules and is skipped.
///
/// What each invocation form must satisfy:
///
/// - **Macro** — the node's bytes begin with the recorded escape character followed
///   by the name as written. A `Spanned` post-space starts immediately after that
///   spelling, and ends where the first child begins; for a childless callable it
///   must merely end no later than the node's span does.
/// - **Specials** — the name is a byte prefix of the node's span, since specials
///   record the name as written. For a paragraph-break node the name is the whole
///   span.
/// - **Environment** — the begin side re-emitted by `write_begin` is a byte prefix
///   of the node's span, and, when the end side was recorded, `write_end` is its byte
///   suffix. What the record re-emits is what was parsed.
// The childless-macro case cannot pin `==`: a takeover's
// `stage_invocation(.., end: Some(&position))` legitimately claims consumed extent
// past the trigger (T5-B / D-plan-17).
fn check_invocation_syntax_payload<LLL: LatexlikeLang, A>(
    tree: &NodeTree<LLL, A>,
    i: usize,
    data: &NodeData<LLL>,
    callable: &CallableData<LLL>,
) {
    let Some(payload) = (&callable.invocation_syntax as &dyn core::any::Any)
        .downcast_ref::<InvocationSyntaxData<StdEnvironmentSyntax<LLL>>>()
    else {
        return;
    };
    let span = data.span.range();
    let source = data.span.source();
    let source_content = source.content();
    let name: &str = &callable.name;
    match payload {
        InvocationSyntaxData::Macro { escape_char, post_space } => {
            let mut spelling = String::new();
            spelling.push(*escape_char);
            spelling.push_str(name);
            assert!(
                source_content.get(span.start..span.start + spelling.len())
                    == Some(spelling.as_str()),
                "node {}: macro spelling {:?} is not the byte prefix of the node's \
                 span {:?}",
                i,
                spelling,
                span
            );
            if let TextContent::Spanned(s) = post_space {
                assert!(
                    s.start() == span.start + spelling.len(),
                    "node {}: spanned post-space {:?} does not follow the macro \
                     spelling (span {:?})",
                    i,
                    s,
                    span
                );
                match tree.nodes_in(data.children.clone()).next() {
                    Some(first) => assert!(
                        s.end() == first.span().start(),
                        "node {}: spanned post-space {:?} does not end at the first \
                         child (starting at {})",
                        i,
                        s,
                        first.span().start()
                    ),
                    None => assert!(
                        s.end() <= span.end,
                        "node {}: spanned post-space {:?} escapes the childless \
                         callable's span {:?}",
                        i,
                        s,
                        span
                    ),
                }
            }
        }
        InvocationSyntaxData::Specials => {
            assert!(
                source_content.get(span.start..span.start + name.len()) == Some(name),
                "node {}: specials name {:?} is not the byte prefix of the node's \
                 span {:?} (name-as-written)",
                i,
                name,
                span
            );
        }
        InvocationSyntaxData::Environment(env) => {
            let node_bytes = &source_content[span.clone()];
            let begin = env.write_begin(name, source);
            assert!(
                node_bytes.starts_with(begin.as_str()),
                "node {}: recorded begin spelling {:?} is not the byte prefix of the \
                 node's span {:?}",
                i,
                begin,
                span
            );
            if env.end.is_some() {
                let end = env.write_end(name, source);
                assert!(
                    node_bytes.ends_with(end.as_str()),
                    "node {}: recorded end spelling {:?} is not the byte suffix of \
                     the node's span {:?}",
                    i,
                    end,
                    span
                );
            }
        }
    }
}

// --- the discriminating pin tests (D-plan-12 Option B home) -------------------------
//
// The positive direction is exercised by every latexlike parse in the crate
// (check_latexlike_tree_invariants runs on them all); these discriminate the pins
// by hand-building trees whose recorded payloads diverge from the bytes.

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec::Vec;

    use super::super::{
        CallableType, GroupType, Latexlike, MacroSpec, SpecialsSpec,
        StdEnvironmentSideSyntax,
    };
    use super::*;
    use crate::node::{
        BuildId, NodeTreeBuilder, ParsedArguments, ParsedSlots,
    };
    use crate::source::{Source, SourceSpan, Span};
    use crate::state::ParsingState;
    use crate::token::GroupRule;

    fn latexlike_state() -> Arc<ParsingState<Latexlike>> {
        Arc::new(ParsingState::lang_initial().expect("seed state"))
    }

    /// A root `List` over `span` holding one callable node of the same span,
    /// with the given payload.
    fn callable_tree(
        content: &str,
        span: core::ops::Range<usize>,
        callable: CallableData<Latexlike>,
    ) -> NodeTree<Latexlike> {
        let source: Arc<Source> = Arc::new(Source::new(content));
        let st = latexlike_state();
        let mut builder: NodeTreeBuilder<Latexlike> = NodeTreeBuilder::new();
        let node = builder.add(
            NodeKind::callable(callable),
            SourceSpan::new(&source, span.clone()),
            Arc::clone(&st),
            Vec::<BuildId>::new(), (), (),
        ).unwrap();
        let root = builder.add(
            NodeKind::list(),
            SourceSpan::new(&source, span),
            Arc::clone(&st),
            alloc::vec![node], (), (),
        ).unwrap();
        builder.finish(root).unwrap()
    }

    #[test]
    #[should_panic(expected = "does not follow the macro spelling")]
    fn rejects_a_macro_post_space_off_the_trigger_spelling() {
        // `\emph x`: the spelling pin puts the post-space at 5..; a recorded
        // 4..6 contradicts the trigger's own extent.
        let tree = callable_tree("\\emph x", 0..7, CallableData {
            callable_type: CallableType::Macro,
            name: "emph".into(),
            spec: Arc::new(MacroSpec::default()),
            arguments: ParsedArguments::empty(),
            slots: ParsedSlots::empty(),
            invocation_syntax: InvocationSyntaxData::Macro {
                escape_char: '\\',
                post_space: TextContent::Spanned(Span::new(4, 6)),
            },
        });
        check_latexlike_tree_invariants(&tree);
    }

    #[test]
    #[should_panic(expected = "macro spelling")]
    fn rejects_a_macro_escape_char_not_in_the_bytes() {
        // The bytes spell `\emph`; the payload claims the `@` escape fired.
        let tree = callable_tree("\\emph x", 0..7, CallableData {
            callable_type: CallableType::Macro,
            name: "emph".into(),
            spec: Arc::new(MacroSpec::default()),
            arguments: ParsedArguments::empty(),
            slots: ParsedSlots::empty(),
            invocation_syntax: InvocationSyntaxData::Macro {
                escape_char: '@',
                post_space: TextContent::Spanned(Span::new(5, 6)),
            },
        });
        check_latexlike_tree_invariants(&tree);
    }

    #[test]
    #[should_panic(expected = "name-as-written")]
    fn rejects_a_specials_name_that_is_not_the_spelling() {
        // The bytes spell `---`; a canonical-key name (`~`) violates
        // name-as-written.
        let tree = callable_tree("a---b", 1..4, CallableData {
            callable_type: CallableType::Specials,
            name: "~".into(),
            spec: Arc::new(SpecialsSpec::<Latexlike>::default()),
            arguments: ParsedArguments::empty(),
            slots: ParsedSlots::empty(),
            invocation_syntax: InvocationSyntaxData::Specials,
        });
        check_latexlike_tree_invariants(&tree);
    }

    #[test]
    #[should_panic(expected = "begin spelling")]
    fn rejects_an_environment_record_diverging_from_the_bytes() {
        // A begin side whose write_begin (`\begin{itemize}`) is nowhere in the
        // node's bytes.
        let begin = StdEnvironmentSideSyntax::<Latexlike> {
            escape_char: '\\',
            command_word: TextContent::from(String::from("begin")),
            post_space: TextContent::empty(),
            name_group_rule: Arc::new(GroupRule {
                group_type: GroupType::Content,
                open: "{".into(),
                close: "}".into(),
            }),
        };
        let tree = callable_tree("xitemizey", 0..9, CallableData {
            callable_type: CallableType::Environment,
            name: "itemize".into(),
            spec: Arc::new(SpecialsSpec::<Latexlike>::default()),
            arguments: ParsedArguments::empty(),
            slots: ParsedSlots::empty(),
            invocation_syntax: InvocationSyntaxData::Environment(StdEnvironmentSyntax {
                begin,
                end: None,
            }),
        });
        check_latexlike_tree_invariants(&tree);
    }
    // --- the gate: a language that does not obey span tiling (PLAN §1.5 R6) ----------

    /// The payload pins are byte accounting, so they hold for a language that obeys
    /// span tiling only. The very tree `rejects_a_macro_escape_char_not_in_the_bytes`
    /// panics on passes the oracle under a language declaring `false` — where the
    /// node's span is what the reader described and pins nothing, while the all-trees
    /// law (which this still runs) holds.
    #[test]
    fn a_language_that_does_not_obey_span_tiling_skips_the_payload_pins() {
        use super::super::test_support::RelaxedLatexlike;

        let source: Arc<Source> = Arc::new(Source::new("\\emph x"));
        let st: Arc<ParsingState<RelaxedLatexlike>> =
            Arc::new(ParsingState::lang_initial().expect("seed state"));
        let mut builder: NodeTreeBuilder<RelaxedLatexlike> = NodeTreeBuilder::new();
        let node = builder
            .add(
                NodeKind::callable(CallableData {
                    callable_type: CallableType::Macro,
                    name: "emph".into(),
                    spec: Arc::new(MacroSpec::default()),
                    arguments: ParsedArguments::empty(),
                    slots: ParsedSlots::empty(),
                    // Neither the escape character nor the spelling is in the node's
                    // bytes — exactly what the pins reject for a tiled language.
                    invocation_syntax: InvocationSyntaxData::Macro {
                        escape_char: '@',
                        post_space: TextContent::Owned("  ".into()),
                    },
                }),
                SourceSpan::new(&source, 0..7),
                Arc::clone(&st),
                Vec::<BuildId>::new(),
                (),
                (),
            )
            .unwrap();
        let root = builder
            .add(
                NodeKind::list(),
                SourceSpan::new(&source, 0..7),
                Arc::clone(&st),
                alloc::vec![node],
                (),
                (),
            )
            .unwrap();
        let tree = builder.finish(root).unwrap();

        crate::node::validate_tree(&tree).expect("the all-trees law holds");
        check_latexlike_tree_invariants(&tree);
    }
}
