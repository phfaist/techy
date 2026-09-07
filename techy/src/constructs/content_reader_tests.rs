//! Tests for the parse-time content readers on [`ParseContext`]:
//! [`content_as_tree`](ParseContext::content_as_tree),
//! [`argument_content_as_tree`](ParseContext::argument_content_as_tree),
//! [`content_as_plain_chars`](ParseContext::content_as_plain_chars) and
//! [`argument_content_as_plain_chars`](ParseContext::argument_content_as_plain_chars).
//!
//! Every test runs a real parse of a language defining `\probe`, a macro whose
//! invocation parser reads each of its own declared arguments through those methods and
//! records what it got. The assertions then run over the recorded values, which is what
//! a crate-external spec author would do inside their own parser.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use std::sync::Mutex;

use super::{
    parse_declared_arguments, ConstructParser, ConstructParserResult, Invocation, ParseContext,
    PlainCharsError,
};
use crate::engine::Language;
use crate::error::{HookFailed, ParseError, Recovery};
use crate::extract::{content_as_chars, split_at_chars_drop_annotations};
use crate::latexlike::{
    argument_specs, CallableType, Latexlike, LatexlikeDriver, MacroSpec,
};
use crate::node::{
    check_tree_invariants, validate_tree, BuildId, ContentNodes, NodeBuildError, NodeTree,
    ParsedArguments, ParsedSlots, StagedNodes,
};
use crate::source::SourceSpan;
use crate::scopes::Package;
use crate::serialize::SerializableObject;
use crate::spec::{ArgumentSpec, CallableSpec, FrameRole};
use crate::state::{ParsingState, ParsingStateDelta};
use crate::token::TokenEdge;

/// What `\probe`'s parser recorded about one declared argument.
#[derive(Debug)]
struct Probe {
    /// The argument's index in the declared list.
    index: usize,
    /// The copy [`ParseContext::argument_content_as_tree`] answered.
    tree: Option<NodeTree<Latexlike, Option<BuildId>>>,
    /// What [`ParseContext::argument_content_as_plain_chars`] answered.
    chars: Result<Option<String>, PlainCharsError>,
    /// The staged ids of the argument's content nodes, worked out independently of
    /// the methods under test, so the annotations can be checked against them.
    content_ids: Vec<BuildId>,
    /// The same, walked recursively in document order: every staged node the copy
    /// covers, content nodes and their descendants alike.
    subtree_ids: Vec<BuildId>,
}

/// What one parse recorded: each argument read once before the invocation was staged,
/// and once again afterwards, when the originals are claimed.
#[derive(Debug, Default)]
struct Probes {
    before: Vec<Probe>,
    after: Vec<Probe>,
}

type Log = Arc<Mutex<Probes>>;

/// The `\probe` macro: the standard declared-argument shape, with an invocation
/// parser that reads its own arguments back before staging.
#[derive(Debug)]
struct ProbeSpec {
    arguments: Vec<Arc<ArgumentSpec<Latexlike>>>,
    log: Log,
}

impl SerializableObject<Latexlike> for ProbeSpec {}

impl CallableSpec<Latexlike> for ProbeSpec {
    fn arguments(&self) -> &[Arc<ArgumentSpec<Latexlike>>] {
        &self.arguments
    }

    fn stack_frame_title(&self, _role: FrameRole, name: &str) -> String {
        alloc::format!("macro \\{name}")
    }

    fn make_invocation_parser<'a>(
        &'a self,
        invocation: Invocation<'a, Latexlike>,
    ) -> Result<
        Box<dyn ConstructParser<Latexlike, Output = BuildId> + 'a>,
        crate::error::ParseError<Option<String>>,
    > {
        Ok(Box::new(ProbeParser { invocation, log: Arc::clone(&self.log) }))
    }
}

struct ProbeParser<'a> {
    invocation: Invocation<'a, Latexlike>,
    log: Log,
}

impl ConstructParser<Latexlike> for ProbeParser<'_> {
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, Latexlike>,
    ) -> ConstructParserResult<Latexlike, (BuildId, Option<Box<ParsingStateDelta<Latexlike>>>)>
    {
        let token = self.invocation.token;
        let name = cx.tokens.source_span_between(token, TokenEdge::Start, TokenEdge::End);
        let (children, arguments) = parse_declared_arguments(cx, self.invocation.spec, &name)?;

        let before = read_all(cx, &arguments, &children)?;
        self.log.lock().unwrap().before.extend(before);

        // The same records and child list, kept for the second read below: staging
        // consumes both, and the point is that reading them again still works.
        let claimed_arguments = arguments.clone();
        let claimed_children = children.clone();

        let id = cx.stage_invocation(
            &self.invocation,
            ParsedArguments::from(arguments),
            ParsedSlots::empty(),
            children,
            None,
        )?;

        // Reading claims nothing and copies everything, so it works just as well once
        // the originals have become the staged callable's children.
        let after = read_all(cx, &claimed_arguments, &claimed_children)?;
        self.log.lock().unwrap().after.extend(after);

        Ok((id, None))
    }
}

/// Reads every declared argument through the four methods under test.
fn read_all(
    cx: &mut ParseContext<'_, '_, Latexlike>,
    arguments: &[crate::node::ParsedArgument<Latexlike>],
    children: &[BuildId],
) -> ConstructParserResult<Latexlike, Vec<Probe>> {
    let mut probes = Vec::with_capacity(arguments.len());
    for (index, argument) in arguments.iter().enumerate() {
        let tree = cx
            .argument_content_as_tree(argument, children)
            .map_err(|error| lift(cx, error, cx.here()))?;
        let chars = cx.argument_content_as_plain_chars(argument, children);
        let content_ids = content_ids(cx, argument, children);
        let subtree_ids = subtree_ids(cx, &content_ids);
        probes.push(Probe { index, tree, chars, content_ids, subtree_ids });
    }
    Ok(probes)
}

/// The documented two-arm lift of a failed read (see
/// [`ParseContext::content_as_tree`]'s Errors section), spelled out the way a
/// crate-external parser must spell it.
fn lift(
    cx: &ParseContext<'_, '_, Latexlike>,
    error: NodeBuildError,
    span: SourceSpan<Option<String>>,
) -> ParseError<Option<String>> {
    match error {
        // The root ext mint's own reported failure: an operational failure in
        // consumer-supplied hook code, not a violated contract.
        NodeBuildError::ExtMintFailed { detail } => {
            ParseError::new(HookFailed::new(detail, None), span)
                .with_frames(cx.session.snapshot_frames())
        }
        // Every other variant is a contract violation by this parser.
        other => cx.implementation_error(other, span),
    }
}

/// The staged ids `roots` and their descendants, in document order — the order the
/// copied tree's [`descendants`](crate::core::node::NodeTree::descendants) walks.
fn subtree_ids(cx: &ParseContext<'_, '_, Latexlike>, roots: &[BuildId]) -> Vec<BuildId> {
    fn walk(staged: &StagedNodes<'_, Latexlike>, id: BuildId, out: &mut Vec<BuildId>) {
        out.push(id);
        if let Some(view) = staged.get(id) {
            for child in view.children() {
                walk(staged, *child, out);
            }
        }
    }
    let staged = cx.staged_nodes();
    let mut out = Vec::new();
    for id in roots {
        walk(&staged, *id, &mut out);
    }
    out
}

/// The argument's content nodes, read straight off the staged record — the
/// independent yardstick the copies' annotations are checked against.
fn content_ids(
    cx: &ParseContext<'_, '_, Latexlike>,
    argument: &crate::node::ParsedArgument<Latexlike>,
    children: &[BuildId],
) -> Vec<BuildId> {
    let Some(region) = argument.region.as_ref() else {
        return Vec::new();
    };
    let (offsets, content) = region.staged().expect("a staged region during the parse");
    match content {
        ContentNodes::InRegion(range) => children
            [offsets.start as usize..offsets.end as usize][range.start as usize..range.end as usize]
            .to_vec(),
        ContentNodes::InChildrenOf(parent, range) => cx
            .staged_nodes()
            .get(*parent)
            .expect("a staged content parent")
            .children()[range.start as usize..range.end as usize]
            .to_vec(),
    }
}

/// A language defining `\probe` with the given argument codes, plus `\emph{…}` for the
/// tests that put a callable inside the probed argument.
fn language(codes: &[&str], recovery: Recovery) -> (Language<Latexlike>, Log) {
    let log: Log = Arc::new(Mutex::new(Probes::default()));
    let mut package = Package::new("content-reader-tests");
    package.insert(
        CallableType::Macro,
        "probe",
        Arc::new(ProbeSpec {
            arguments: argument_specs(codes).unwrap(),
            log: Arc::clone(&log),
        }),
    );
    package.insert(
        CallableType::Macro,
        "emph",
        Arc::new(MacroSpec::new(argument_specs(["m"]).unwrap())),
    );
    let language = Language::new(
        LatexlikeDriver::new(recovery),
        ParsingState::lang_initial_with_packages([package]).expect("seed state"),
    )
    .with_descent_guard_init(crate::engine::StdDescentGuardInit::depth_limit(64));
    (language, log)
}

/// Parses `input` under a `\probe` declaring `codes`, and returns what the parser
/// recorded. The parse itself must be clean.
fn probe_both(codes: &[&str], input: &str) -> Probes {
    let (language, log) = language(codes, Recovery::Strict);
    let result = language.parse(input).unwrap();
    check_tree_invariants(&result.tree);
    assert!(result.diagnostics.is_empty(), "unexpected diagnostics: {:?}", result.diagnostics);
    let probes = core::mem::take(&mut *log.lock().unwrap());
    probes
}

/// The reads taken *before* the invocation was staged — what most tests here look at.
fn probe(codes: &[&str], input: &str) -> Vec<Probe> {
    probe_both(codes, input).before
}

/// The annotations of the copied top-level content nodes, in order.
fn annotations(tree: &NodeTree<Latexlike, Option<BuildId>>) -> Vec<Option<BuildId>> {
    tree.root().children().iter().map(|node| *node.annotation()).collect()
}

/// Every non-root annotation of the copy, in document order — one per staged node the
/// copy covers.
fn all_annotations(tree: &NodeTree<Latexlike, Option<BuildId>>) -> Vec<Option<BuildId>> {
    tree.descendants().map(|node| *node.annotation()).collect()
}

#[test]
fn a_delimited_argument_reads_as_the_groups_children() {
    // `{…}` content is an `InChildrenOf` designation: the group's children, braces
    // excluded.
    let probes = probe(&["m"], r"\probe{a \emph{b} c}");
    assert_eq!(probes.len(), 1);
    let probe = &probes[0];
    assert_eq!(probe.index, 0);
    let tree = probe.tree.as_ref().expect("a provided argument answers a tree");
    check_tree_invariants(tree);

    // The root is the synthesized `List`; its span covers the content, braces excluded.
    assert!(tree.root().is_list());
    assert_eq!(tree.root().span_content(), "a \\emph{b} c");
    assert_eq!(tree.root().child_count(), 3);
    assert_eq!(tree.root().child(0).unwrap().chars(), Some("a "));
    assert_eq!(tree.root().child(1).unwrap().macro_name(), Some("emph"));

    // The root is synthesized, so it has no original; every copy names one.
    assert_eq!(*tree.root().annotation(), None);
    assert_eq!(annotations(tree), probe.content_ids.iter().copied().map(Some).collect::<Vec<_>>());
    // The mapping runs the whole way down, not just at the top level: every copied
    // node, at any depth, names the staged node it came from.
    assert_eq!(
        all_annotations(tree),
        probe.subtree_ids.iter().copied().map(Some).collect::<Vec<_>>()
    );
    assert!(probe.subtree_ids.len() > probe.content_ids.len(), "the copy has depth");
    let emph_argument = tree.root().child(1).unwrap().argument_content_nodes(0).unwrap();
    assert_eq!(emph_argument.first().unwrap().chars(), Some("b"));

    // The copy carries a callable, so it is not plain characters.
    assert!(matches!(probe.chars, Err(PlainCharsError::NotPlainCharacters { .. })));
}

#[test]
fn a_bare_single_token_argument_reads_as_that_token() {
    // `\probe 1` designates its content `InRegion`: the single staged chars node,
    // with no group around it.
    let probes = probe(&["m"], r"\probe 1x");
    let probe = &probes[0];
    let tree = probe.tree.as_ref().unwrap();
    check_tree_invariants(tree);
    assert_eq!(tree.root().child_count(), 1);
    assert_eq!(tree.root().span_content(), "1");
    assert_eq!(tree.root().child(0).unwrap().chars(), Some("1"));
    assert_eq!(annotations(tree), [Some(probe.content_ids[0])]);
    assert_eq!(probe.chars.as_ref().unwrap().as_deref(), Some("1"));
}

#[test]
fn empty_content_answers_an_empty_tree_anchored_in_the_group() {
    // `\probe{}` is *provided* with empty content: a tree with no child, whose root
    // still says where that empty content sits.
    let probes = probe(&["m"], r"\probe{}");
    let probe = &probes[0];
    let tree = probe.tree.as_ref().expect("a provided argument answers a tree");
    // A well-formed tree by the all-trees law. The span-tiling oracle does not apply:
    // the root's span says *where* the empty content sits — inside the group — rather
    // than covering nodes there are none of (the documented fallback).
    assert!(validate_tree(tree).is_ok(), "{:?}", validate_tree(tree));
    assert_eq!(tree.root().child_count(), 0);
    assert_eq!(tree.root().span_content(), "{}");
    assert_eq!(*tree.root().annotation(), None);
    assert_eq!(probe.chars.as_ref().unwrap().as_deref(), Some(""));
}

#[test]
fn an_argument_that_was_not_provided_answers_none() {
    // The optional is absent, the mandatory one is there: `Ok(None)` distinguishes
    // "not provided" from "provided with empty content".
    let probes = probe(&["o", "m"], r"\probe{x}");
    assert_eq!(probes.len(), 2);
    assert!(probes[0].tree.is_none());
    assert_eq!(probes[0].chars.as_ref().unwrap(), &None);
    assert!(probes[1].tree.is_some());
    assert_eq!(probes[1].chars.as_ref().unwrap().as_deref(), Some("x"));
}

#[test]
fn reading_works_just_as_well_after_the_invocation_is_staged() {
    // Reading copies and claims nothing, so `stage_invocation` taking the originals as
    // the callable's children changes nothing about what a later read answers.
    let probes = probe_both(&["o", "m"], r"\probe[opt]{a \emph{b} c}");
    assert_eq!(probes.before.len(), 2);
    assert_eq!(probes.after.len(), 2);

    for (before, after) in probes.before.iter().zip(&probes.after) {
        assert_eq!(before.index, after.index);
        assert_eq!(before.chars, after.chars);
        assert_eq!(before.content_ids, after.content_ids);
        assert_eq!(before.subtree_ids, after.subtree_ids);
        match (&before.tree, &after.tree) {
            (Some(before), Some(after)) => {
                assert_eq!(
                    after.descendants().map(|n| n.span_content()).collect::<Vec<_>>(),
                    before.descendants().map(|n| n.span_content()).collect::<Vec<_>>()
                );
                assert_eq!(all_annotations(after), all_annotations(before));
                assert_eq!(*after.root().annotation(), None);
                assert_eq!(after.root().span_content(), before.root().span_content());
            }
            (None, None) => {}
            other => panic!("the two reads disagree on provision: {other:?}"),
        }
    }
    // And the reads are the real ones, not two empty answers: the optional argument's
    // characters, and the mandatory one's refusal at the same staged node.
    assert_eq!(probes.after[0].chars.as_ref().unwrap().as_deref(), Some("opt"));
    assert!(matches!(probes.after[1].chars, Err(PlainCharsError::NotPlainCharacters { .. })));
    assert!(!probes.after[1].subtree_ids.is_empty());
}

#[test]
fn the_copied_tree_feeds_the_extract_helpers() {
    // The point of copying: every reading helper of `techy::extract` applies at parse
    // time, over `tree.root().children()`.
    let probes = probe(&["m"], r"\probe{key1,key2,my{special,key},key4}");
    let tree = probes[0].tree.as_ref().unwrap();

    // `content_as_chars` descends into the group, so its delimiters do not show.
    assert_eq!(
        content_as_chars(tree.root().children()).unwrap(),
        "key1,key2,myspecial,key,key4"
    );

    let split = split_at_chars_drop_annotations(tree.root().children(), ",").unwrap();
    assert_eq!(split.segments().count(), 4);
    let texts: Vec<String> = split
        .segments()
        .map(|segment| content_as_chars(segment).unwrap().to_string())
        .collect();
    assert_eq!(texts, ["key1", "key2", "myspecial,key", "key4"]);
}

#[test]
fn plain_chars_reads_characters_and_refuses_everything_else() {
    // The strict rule: characters and nothing but.
    assert_eq!(
        probe(&["m"], r"\probe{chap.tex}")[0].chars.as_ref().unwrap().as_deref(),
        Some("chap.tex")
    );
    // Whitespace is content like any other character — nothing is trimmed.
    assert_eq!(
        probe(&["m"], r"\probe{ chap.tex }")[0].chars.as_ref().unwrap().as_deref(),
        Some(" chap.tex ")
    );

    // A protective group inside the argument: the content is one group node.
    let probes = probe(&["m"], r"\probe{{chap.tex}}");
    let group = probes[0].content_ids[0];
    assert_eq!(probes[0].chars, Err(PlainCharsError::NotPlainCharacters { node: group }));
    // The tree reader has no such rule — it copies whatever is there.
    assert!(probes[0].tree.as_ref().unwrap().root().child(0).unwrap().is_group());

    // A comment among the content.
    let probes = probe(&["m"], "\\probe{chap% a note\n.tex}");
    let error = probes[0].chars.as_ref().unwrap_err();
    assert!(matches!(error, PlainCharsError::NotPlainCharacters { .. }));
    // The offending node is named, and it is the comment (the second content node).
    assert_eq!(
        error,
        &PlainCharsError::NotPlainCharacters { node: probes[0].content_ids[1] }
    );

    // A callable among the content.
    let probes = probe(&["m"], r"\probe{\emph{x}}");
    assert_eq!(
        probes[0].chars,
        Err(PlainCharsError::NotPlainCharacters { node: probes[0].content_ids[0] })
    );

    // The rendered wording names the node.
    let rendered = probes[0].chars.as_ref().unwrap_err().to_string();
    assert!(rendered.contains("is not plain characters"), "unexpected wording: {rendered}");
}

#[test]
fn the_tree_reader_flattens_where_the_plain_reader_refuses() {
    // The documented way to get the flattening rule instead of the strict one:
    // `extract::content_as_chars` over the copied tree descends into the group.
    let probes = probe(&["m"], r"\probe{{chap.tex}}");
    let tree = probes[0].tree.as_ref().unwrap();
    assert_eq!(content_as_chars(tree.root().children()).unwrap(), "chap.tex");
}
