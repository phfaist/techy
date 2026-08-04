//! The latexlike parse-law checker (test builds only): the core parse-tree law
//! plus the preset's **invocation-syntax payload pins** — the byte checks that
//! pin the recorded trigger spellings ([`InvocationSyntaxData`]) against the
//! node's own bytes.
//!
//! Core's [`check_tree_invariants`] is deliberately **payload-blind**: the
//! invocation-syntax payload is Lang-owned, and core cannot pin facts it cannot
//! read (the D-plan-12 Option B ruling — the pins live with the preset that owns
//! the payload types). One call to [`check_latexlike_tree_invariants`] runs both:
//! the core parse law, then the payload pins for every callable whose payload is
//! the family enum over the family's standard environment record
//! ([`InvocationSyntaxData<StdEnvironmentSyntax<LLL>>`]); a custom `Env` type is
//! its language's own recording discipline and is skipped.
//!
//! Mechanism mirror of the core checker: `pub(crate)` + `#[cfg(test)]` — an
//! in-crate test oracle, not builder law (integration tests use the public
//! [`validate_tree`](crate::node::validate_tree), which never carried the pins).

use alloc::string::String;

use crate::node::{
    check_tree_invariants, CallableData, NodeData, NodeKind, NodeTree,
};
use crate::source::TextContent;

use super::invocation_syntax::{
    EnvironmentSyntax, InvocationSyntaxData, StdEnvironmentSyntax,
};
use super::lang::LatexlikeLang;

/// Check a finished latexlike-family tree against the **parse-tree law plus the
/// payload pins**: the core [`check_tree_invariants`] (all-trees law + byte
/// accounting), then the invocation-syntax payload pins below. Panics with a
/// description of the first violation — the in-crate test oracle for every tree a
/// latexlike-family parse produces.
pub(crate) fn check_latexlike_tree_invariants<LLL: LatexlikeLang, A>(
    tree: &NodeTree<LLL, A>,
) {
    check_tree_invariants(tree);
    for (i, data) in tree.nodes().iter().enumerate() {
        if let NodeKind::Callable(callable) = &data.kind {
            check_invocation_syntax_payload(tree, i, data, callable);
        }
    }
}

/// The invocation-syntax payload pins (the payload arm of
/// [`check_latexlike_tree_invariants`]): reads the family payload — the
/// [`InvocationSyntaxData`] enum over the family's standard environment record —
/// via `Any` downcast, and checks the recorded spellings against the node's
/// bytes. A payload of any other type (a custom `Env`, a foreign record) is its
/// language's own recording discipline and is skipped.
///
/// The pins, per arm:
///
/// - **Macro** — the spelling fact: the node's bytes begin with the recorded
///   escape character followed by the name as written; a `Spanned` post-space
///   starts right after that spelling and ends where the first child begins — or
///   at most at the span's end for a childless callable (`==` cannot be pinned
///   there: a takeover's `stage_invocation(.., end_pos: Some)` legitimately
///   claims consumed extent past the trigger, T5-B / D-plan-17).
/// - **Specials** — name-as-written: the name is a byte prefix of the node's
///   span (for paragraph-break `Specials` nodes the name is the whole span).
/// - **Environment** — `write_begin` is a byte prefix of the node's span slice;
///   when the end side is recorded, `write_end` is its byte suffix (the accuracy
///   doctrine made mechanical: what the record reemits is what was parsed).
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
        Arc::new(ParsingState::lang_initial())
    }

    /// A root `List` over `span` holding one callable node of the same span,
    /// carrying the given payload.
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
}
