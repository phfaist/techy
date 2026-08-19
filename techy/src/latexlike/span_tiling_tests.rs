//! Preset-side parse tests for a language whose reader serves one parse from **several
//! sources** — a member of the latexlike family declaring [`Lang::OBEYS_SPAN_TILING`]
//! `= false`.
//!
//! [`RelaxedScriptedLatexlike`] is the demonstration the plan's rule R7 asks for: the
//! preset's parsers, driver, specs and node-ext types are generic over the family, so
//! they serve a member over a *custom tokenization* ([`ScriptedTokenization`], the
//! multi-source test reader) unchanged. Nothing in the preset had to be relaxed to
//! instantiate it.
//!
//! The scripts are written as in `constructs::span_tiling_tests`: segments in stream
//! order, each a source and a byte range of it, with a **seam** wherever the next token
//! comes from a different source than the previous one. `Language::parse` is not the
//! route here — a script is runtime data and
//! [`Tokenization::make_token_reader`](crate::token::Tokenization::make_token_reader)
//! sees only a source — so the tests build the reader and run the preset's content loop
//! through a [`ParseContext`], exactly as the two-reader agreement suites drive parsers.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;

use crate::constructs::{NodesParser, ParseContext, StopSpec};
use crate::engine::{ParseResult, ParserSession};
use crate::error::{ParseError, Recovery};
use crate::node::{NodeBuildError, NodeKind, NodeRef, StagedChildren};
use crate::recompose::TreeRecomposer;
use crate::scopes::Package;
use crate::source::{MapResolver, Source, SourceSpan, TextContent};
use crate::state::{Lang, ParsingState};
use crate::token::{ScriptedReader, ScriptedTokenization};

use super::{
    check_latexlike_tree_invariants, source_recomposer, CallableType, GroupType,
    LatexlikeDriver, LatexlikeLang, Mode,
};

/// A member of the latexlike family that reads its tokens from the scripted
/// multi-source reader ([`ScriptedTokenization`]) and therefore declares that its parse
/// trees are **not** span-tiled ([`Lang::OBEYS_SPAN_TILING`] `= false`).
///
/// Everything but those two lines is the preset's own: the vocabularies, the seed, the
/// driver, the node-ext and invocation-syntax types.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RelaxedScriptedLatexlike;

impl Lang for RelaxedScriptedLatexlike {
    const OBEYS_SPAN_TILING: bool = false;

    type Features = crate::state::AllLangFeatures;
    type GroupTypeId = GroupType;
    type CallableTypeId = CallableType;
    type ModeId = Mode;
    type StateExt = ();
    type Event = super::Event;
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type Tokenization = ScriptedTokenization;
    type NodeExts = super::LatexlikeNodeExts;
    type InvocationSyntax =
        super::InvocationSyntaxData<super::StdEnvironmentSyntax<Self>>;
    type Driver = LatexlikeDriver<Self>;

    /// The preset's own seed, for this language's vocabularies.
    fn initial_state_data() -> Result<crate::state::StateData<Self>, crate::state::FinalizeError>
    {
        let mut scopes = crate::scopes::ScopeStack::new();
        scopes.push(super::builtin_package());
        Ok(crate::state::StateData {
            rules: super::default_token_rules(),
            scopes,
            mode: Mode::Text,
            ext: (),
        })
    }

    fn scan_specials(
        state: &ParsingState<Self>,
        content: &str,
        pos: usize,
    ) -> Result<Option<crate::token::SpecialsMatch<Self>>, crate::token::SpecialsScanError>
    {
        state.scopes().scan_specials(state, content, pos)
    }

    fn specials_trigger_chars(
        data: &crate::state::StateData<Self>,
    ) -> crate::token::TriggerChars {
        data.scopes.specials_trigger_chars()
    }

    fn make_node_ext(
        _kind: &NodeKind<Self>,
        _span: &SourceSpan<Self::SourceOrigin>,
        _state: &Arc<ParsingState<Self>>,
        _children: StagedChildren<'_, Self>,
    ) -> Result<(), NodeBuildError> {
        Ok(())
    }
}

impl LatexlikeLang for RelaxedScriptedLatexlike {}

const _: () = assert!(!RelaxedScriptedLatexlike::OBEYS_SPAN_TILING);

/// A primary source.
fn source(content: &str) -> Arc<Source> {
    Arc::new(Source::new(content))
}

/// One segment of a script, as the tests write them.
type Segment<'s> = (&'s Arc<Source>, Range<usize>);

/// The seed state of [`RelaxedScriptedLatexlike`] with `package` pushed innermost.
fn seed(package: Package<RelaxedScriptedLatexlike>) -> Arc<ParsingState<RelaxedScriptedLatexlike>> {
    Arc::new(ParsingState::lang_initial_with_packages([Arc::new(package)]).expect("seed state"))
}

/// Run the preset's content loop over the scripted stream of `segments` under `state`,
/// stage the nodes under a root `List` spanning what the reader describes for the whole
/// parse, freeze, and run both tree oracles — the all-trees law and the preset's
/// span-tiling oracle, which for this language runs the all-trees law and stops there
/// (plan test **T10**).
fn run_script(
    state: &Arc<ParsingState<RelaxedScriptedLatexlike>>,
    segments: &[Segment<'_>],
    recovery: Recovery,
) -> Result<ParseResult<RelaxedScriptedLatexlike>, ParseError> {
    run_script_with(state, segments, LatexlikeDriver::new(recovery))
}

/// [`run_script`] with a driver the caller configured (an `\input` source resolver).
fn run_script_with(
    state: &Arc<ParsingState<RelaxedScriptedLatexlike>>,
    segments: &[Segment<'_>],
    driver: LatexlikeDriver<RelaxedScriptedLatexlike>,
) -> Result<ParseResult<RelaxedScriptedLatexlike>, ParseError> {
    let mut reader = ScriptedReader::<RelaxedScriptedLatexlike>::new(segments, state);
    let mut session = ParserSession::new();
    let (nodes, root_span) = {
        let mut cx = ParseContext::new(&mut reader, Arc::clone(state), &mut session, &driver);
        let begin = cx.tokens.position_here();
        let mut parser = NodesParser::new(StopSpec::none());
        let (outcome, delta) = crate::constructs::ConstructParser::parse(&mut parser, &mut cx)?;
        assert!(delta.is_none(), "a nodes parser reports no pass-through delta");
        let end = cx.tokens.position_here();
        let root_span = cx.source_span_within(&begin, &end)?;
        (outcome.nodes, root_span)
    };
    let kind = NodeKind::list();
    // The root's node ext, minted through the language's own hook exactly as
    // `ParseContext::stage_node` would (`()` for this language, hence no binding).
    RelaxedScriptedLatexlike::make_node_ext(
        &kind,
        &root_span,
        state,
        session.builder.staged_children(&nodes),
    )
    .expect("mint the root's node ext");
    let root = session
        .builder
        .add(kind, root_span, Arc::clone(state), nodes, (), ())
        .expect("stage the root list");
    let result = session.finish(root).expect("freeze the tree");
    crate::node::validate_tree(&result.tree).expect("the all-trees law holds");
    check_latexlike_tree_invariants(&result.tree);
    Ok(result)
}

/// Re-emit a tree through the preset's source recomposer — "as stored", which for this
/// language is the only thing re-emission can mean (no byte equality with any source is
/// claimed).
fn reemit(result: &ParseResult<RelaxedScriptedLatexlike>) -> String {
    TreeRecomposer::new(&mut source_recomposer::<RelaxedScriptedLatexlike>())
        .recompose(&result.tree, ())
        .expect("the preset re-emits the tree")
}

/// The `List` node of an environment's body slot, and its content nodes.
fn body_nodes<'t>(
    environment: NodeRef<'t, RelaxedScriptedLatexlike>,
) -> Vec<NodeRef<'t, RelaxedScriptedLatexlike>> {
    environment
        .body()
        .expect("the environment has a body slot")
        .iter()
        .collect()
}

/// A package defining `itemize` as a no-argument environment.
fn itemize_package() -> Package<RelaxedScriptedLatexlike> {
    let mut package = Package::new("relaxed-scripted-envs");
    package
        .define_environment("itemize", core::iter::empty::<&str>())
        .expect("no argument codes");
    package
}

// --- T4: an environment whose scaffolding and body lie in different sources -----------

#[test]
fn an_environment_spanning_seams_is_built_and_reemitted_as_stored() {
    // `A[0..15]` = `"\\begin{itemize}"`, `B[0..4]` = `"body"`, `A[15..28]` =
    // `"\\end{itemize}"` — the plan's script: the scaffolding in one source, the body
    // in another.
    let a = source("\\begin{itemize}\\end{itemize}");
    let b = source("body");
    let state = seed(itemize_package());
    let result = run_script(&state, &[(&a, 0..15), (&b, 0..4), (&a, 15..28)], Recovery::Strict)
        .expect("the parse runs");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);

    let root = result.tree.root();
    assert_eq!(root.children().len(), 1);
    let environment = root.child(0).expect("the environment node");
    assert_eq!(environment.name(), Some("itemize"));
    assert_eq!(environment.environment_name(), Some("itemize"));
    // The node's span is what the reader described for the whole invocation: the
    // scaffolding's source, from the trigger to where the stream left that source.
    assert!(Arc::ptr_eq(environment.span().source(), &a));
    assert_eq!(environment.span().range(), 0..28);

    // The body's content crossed two seams, so it is recorded as the text read.
    let body = body_nodes(environment);
    assert_eq!(body.len(), 1);
    match body[0].kind() {
        NodeKind::Chars { content: TextContent::Owned(text) } => assert_eq!(&text[..], "body"),
        other => panic!("expected owned body content, found {other:?}"),
    }
    assert_eq!(body[0].chars(), Some("body"));
    assert!(Arc::ptr_eq(body[0].span().source(), &b));
    assert_eq!(body[0].span().range(), 0..4);

    // Both scaffolding sides lie in the node's own source, so the node-data rule keeps
    // them span-backed — the same rule, the other arm from the test below.
    let side = |side: &super::StdEnvironmentSideSyntax<RelaxedScriptedLatexlike>| {
        assert!(
            matches!(side.command_word, TextContent::Spanned(_)),
            "the command word lies in the node's own source: {:?}",
            side.command_word
        );
    };
    let syntax = environment_syntax(&environment);
    side(&syntax.begin);
    side(syntax.end.as_ref().expect("the terminator was consumed"));

    // Re-emission reads the recorded payloads only: the scaffolding as recorded, the
    // body as stored.
    assert_eq!(reemit(&result), "\\begin{itemize}body\\end{itemize}");
}

#[test]
fn an_environment_terminator_from_another_source_is_recorded_as_text() {
    // `A[0..15]` = `"\\begin{itemize}"`, `B[0..17]` = `"body\\end{itemize}"`: the
    // node's span is described in `A`, so the terminator's spelling facts — which lie
    // in `B` — cannot be span-backed. The node-data rule records them as text, and
    // re-emission is unaffected.
    let a = source("\\begin{itemize}");
    let b = source("body\\end{itemize}");
    let state = seed(itemize_package());
    let result = run_script(&state, &[(&a, 0..15), (&b, 0..17)], Recovery::Strict)
        .expect("the parse runs");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);

    let environment = result.tree.root().child(0).expect("the environment node");
    assert!(Arc::ptr_eq(environment.span().source(), &a));
    assert_eq!(environment.span().range(), 0..15);

    let syntax = environment_syntax(&environment);
    let end = syntax.end.as_ref().expect("the terminator was consumed");
    match &end.command_word {
        TextContent::Owned(word) => assert_eq!(&word[..], "end"),
        other => panic!("the terminator comes from another source: {other:?}"),
    }
    assert_eq!(reemit(&result), "\\begin{itemize}body\\end{itemize}");
}

#[test]
fn an_environment_name_read_across_a_seam_resolves_the_environment() {
    // `A[0..7]` = `"\\begin{"`, `B[0..7]` = `"itemize"`, `A[7..22]` =
    // `"}x\\end{itemize}"`: the name group's delimiters and its characters lie in two
    // sources, so the span the reader describes for the name is not the name. The
    // parser reads the characters as it consumes them, which is what the lookup, the
    // node's `name` and the diagnostics see.
    let a = source("\\begin{}x\\end{itemize}");
    let b = source("itemize");
    let state = seed(itemize_package());
    let result = run_script(&state, &[(&a, 0..7), (&b, 0..7), (&a, 7..22)], Recovery::Tolerant)
        .expect("the parse runs");
    assert!(
        result.diagnostics.is_empty(),
        "the lookup found the environment: {:?}",
        result.diagnostics
    );

    let environment = result.tree.root().child(0).expect("the environment node");
    assert_eq!(environment.name(), Some("itemize"));
    let body = body_nodes(environment);
    assert_eq!(body.len(), 1);
    assert_eq!(body[0].chars(), Some("x"));
    assert_eq!(reemit(&result), "\\begin{itemize}x\\end{itemize}");
}

/// The environment record of a `Callable` node built by the preset.
fn environment_syntax<'t>(
    node: &NodeRef<'t, RelaxedScriptedLatexlike>,
) -> &'t super::StdEnvironmentSyntax<RelaxedScriptedLatexlike> {
    match &node.callable().expect("a callable node").invocation_syntax {
        super::InvocationSyntaxData::Environment(env) => env,
        other => panic!("expected an environment payload, found {other:?}"),
    }
}

// --- T8: the macro post-space payload --------------------------------------------------

#[test]
fn a_macro_post_space_is_recorded_as_text_where_the_language_does_not_obey_tiling() {
    // `B[0..1]` = `"y"`, `A[0..6]` = `"\\foo z"`: the trigger arrives after a seam,
    // and its syntactic post-space is a reader answer recorded *before* the node's span
    // is known — so it cannot go through the node-data rule, and the text is what is
    // recorded.
    let a = source("\\foo z");
    let b = source("y");
    let mut package = Package::new("relaxed-scripted-macros");
    package.define_macro("foo", core::iter::empty::<&str>()).expect("no argument codes");
    let state = seed(package);
    let result = run_script(&state, &[(&b, 0..1), (&a, 0..6)], Recovery::Strict)
        .expect("the parse runs");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);

    let root = result.tree.root();
    let shapes: Vec<String> = root.children().iter().map(|node| node.summary()).collect();
    assert_eq!(shapes, ["chars(y)", "Macro(foo)", "chars(z)"], "{shapes:?}");

    let macro_node = root.child(1).expect("the macro node");
    assert!(Arc::ptr_eq(macro_node.span().source(), &a));
    assert_eq!(macro_node.span().range(), 0..5);
    match &macro_node.callable().expect("a callable node").invocation_syntax {
        super::InvocationSyntaxData::Macro { escape_char, post_space } => {
            assert_eq!(*escape_char, '\\');
            match post_space {
                TextContent::Owned(text) => assert_eq!(&text[..], " "),
                other => panic!("expected owned post-space, found {other:?}"),
            }
        }
        other => panic!("expected a macro payload, found {other:?}"),
    }
    assert_eq!(reemit(&result), "y\\foo z");
}

// --- the `\input` reference, read across a seam ---------------------------------------

/// A package defining `\input` under the shipped registration recipe.
fn input_package() -> Package<RelaxedScriptedLatexlike> {
    let mut package = Package::new("relaxed-scripted-inputs");
    package.insert(
        CallableType::Macro,
        "input",
        super::input_macro_spec::<RelaxedScriptedLatexlike>(false, super::BodyMarker::not_body()),
    );
    package
}

/// A driver resolving `chap.tex` to `content`.
fn resolving_driver(content: &str) -> LatexlikeDriver<RelaxedScriptedLatexlike> {
    let mut resolver = MapResolver::new();
    resolver.insert("chap.tex", content);
    LatexlikeDriver::new(Recovery::Tolerant)
        .with_source_resolver(resolver.with_reference_as_origin())
}

#[test]
fn an_input_reference_read_across_seams_resolves() {
    // `A[0..7]` = `"\\input{"`, `B[0..8]` = `"chap.tex"`, `A[7..8]` = `"}"`. The
    // reference drives source resolution, so it must be exact: `\input` reads it out of
    // the staged argument's own node data (chars payloads), never off the span the
    // reader described for the stretch — which here covers `B` alone while the
    // invocation's span lies in `A`.
    let a = source("\\input{}");
    let b = source("chap.tex");
    let state = seed(input_package());
    let result = run_script_with(
        &state,
        &[(&a, 0..7), (&b, 0..8), (&a, 7..8)],
        resolving_driver("included"),
    )
    .expect("the parse runs");
    assert!(
        result.diagnostics.is_empty(),
        "the reference resolved: {:?}",
        result.diagnostics
    );

    let input = result.tree.root().child(0).expect("the input node");
    assert_eq!(input.name(), Some("input"));
    let reference: Vec<_> = input
        .argument_content_nodes_named("reference")
        .expect("a named argument")
        .expect("the argument is provided")
        .iter()
        .collect();
    assert_eq!(reference.len(), 1);
    match reference[0].kind() {
        NodeKind::Chars { content: TextContent::Owned(text) } => {
            assert_eq!(&text[..], "chap.tex")
        }
        other => panic!("expected owned reference content, found {other:?}"),
    }

    // A source *was* attached — which is the proof that the reference the parser read
    // is the one the resolver knows. Its content parses empty here: the attached
    // sub-parse builds its reader through the language's own tokenization, and a
    // scripted tokenization has no script to give it (documented on
    // `ScriptedTokenization`).
    let slots = input.slots().expect("a callable node");
    assert_eq!(slots.len(), 1, "the resolved source was attached");
    assert_eq!(slots.get(0).expect("the attached slot").name(), Some("attached"));
}

#[test]
fn an_input_reference_that_is_not_plain_characters_is_not_read() {
    // The documented `None` answer of the reference read: content that is not plain
    // characters (here a protective group, `\input{{chap.tex}}`) has no single text in
    // its node data, and a reference taken from a coordinate span would be a guess. The
    // reference is therefore not read at all — nothing is resolved and nothing is
    // attached.
    let a = source("\\input{{chap.tex}}");
    let state = seed(input_package());
    let result = run_script_with(&state, &[(&a, 0..18)], resolving_driver("included"))
        .expect("the parse runs");

    let input = result.tree.root().child(0).expect("the input node");
    assert_eq!(input.name(), Some("input"));
    let reference: Vec<_> = input
        .argument_content_nodes_named("reference")
        .expect("a named argument")
        .expect("the argument is provided")
        .iter()
        .collect();
    assert_eq!(reference.len(), 1);
    assert!(reference[0].is_group(), "the content is a group, not characters");
    // The argument is present and well-formed, so nothing diagnoses it: not reading a
    // reference here is silent.
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(
        input.slots().is_none_or(|slots| slots.is_empty()),
        "no source is attached when the reference cannot be read"
    );
}
