//! Parse tests for a language whose reader serves one parse from **several sources** —
//! a language declaring [`Lang::OBEYS_SPAN_TILING`] `= false`.
//!
//! The reader is the [`ScriptedReader`]: a script is a list of segments (a source and a
//! byte range of it), and the tokens of all segments, concatenated, are the stream. A
//! **seam** is a place in the stream where the next token comes from a different source
//! than the previous one. Each test states its script in the shape
//! `A[0..1]`, `B[0..3]`, `A[5..7]` — the segments in stream order — and asserts:
//!
//! - the tree's structure (kinds, nesting, child counts);
//! - how each fact is **recorded** — [`TextContent::Owned`] where the parser could not
//!   prove the fact lies in the node's own source, [`TextContent::Spanned`] where it
//!   could (the node-data rule);
//! - the spans exactly as the parsers recorded them (the reader's *describing* span for
//!   a node covering several tokens);
//! - the all-trees law ([`validate_tree`](crate::node::validate_tree)) and the
//!   test-only span-tiling oracle
//!   ([`check_tree_invariants`](crate::node::check_tree_invariants), which holds such a
//!   tree to the all-trees law only) — both run by [`run_nodes`] for every tree here.
//!
//! Several tests run the *same script* under a language that **does** obey span tiling
//! ([`TiledScriptedLang`]) and assert the implementation error the machinery reports —
//! the enforcement that makes the declaration meaningful.
//!
//! The preset-side counterparts (an environment spanning seams, the macro post-space
//! payload) live in `latexlike::span_tiling_tests`.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::ops::Range;

use crate::engine::{ParseResult, ParserSession, StdParseDriver};
use crate::error::{ParseError, Recovery};
use crate::node::{BuildId, NodeBuildError, NodeKind, NodeRef, StagedChildren};
use crate::recompose::{core_source_instruction, Recompose, RecomposeContext, Recomposer, TreeRecomposer};
use crate::scopes::ScopeStack;
use crate::source::{Source, SourceSpan, TextContent};
use crate::spec::ArgumentSpec;
use crate::state::{AllLangFeatures, Lang, ParsingState, StateData};
use crate::token::{
    CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
    GroupRules, ParagraphRules, RelaxedLang, ScriptedReader, ScriptedTokenization,
    SpecialsRules, TokenEdge, TokenKind, TokenReader, TokenRules, WhitespaceRules,
};

use super::{
    ConstructParser, ImplementationError, NodesParser, OptionalGroupArgumentParser,
    ParseContext, StopCause, StopSpec, TokenStopKind, VerbatimArgumentParser,
};

/// The tiled twin of [`RelaxedLang`]: the same tokenization (the scripted reader) and
/// the same associated types, with [`Lang::OBEYS_SPAN_TILING`] left at its default
/// (`true`). Running one script under both languages is what pins the enforcement: the
/// machinery must report an implementation error wherever the stream breaks contract
/// clause 8.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TiledScriptedLang;

impl Lang for TiledScriptedLang {
    type Features = AllLangFeatures;
    type GroupTypeId = u32;
    type CallableTypeId = u32;
    type ModeId = ();
    type StateExt = ();
    type Event = ();
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type Tokenization = ScriptedTokenization;
    type NodeExts = ();
    type InvocationSyntax = ();
    type Driver = StdParseDriver;

    fn make_node_ext(
        _kind: &NodeKind<Self>,
        _span: &SourceSpan<Self::SourceOrigin>,
        _state: &Arc<ParsingState<Self>>,
        _children: StagedChildren<'_, Self>,
    ) -> Result<(), NodeBuildError> {
        Ok(())
    }
}

const _: () = assert!(!RelaxedLang::OBEYS_SPAN_TILING);
const _: () = assert!(TiledScriptedLang::OBEYS_SPAN_TILING);

/// The group class of `{`…`}` in the test rules.
const GT_BRACE: u32 = 0;
/// The group class of `[`…`]` in the test rules — the optional-argument delimiters of
/// the backtracking test, listed in the *seed* rules so a fixed script tokenizes them
/// (a scripted stream cannot be re-tokenized under a momentary state).
const GT_BRACKET: u32 = 1;

/// The token rules every script here is tokenized under: whitespace, paragraph breaks,
/// `{`…`}` and `[`…`]` groups, `\`-led commands, `%` comments.
fn script_rules<L>() -> TokenRules<L>
where
    L: Lang<Features = AllLangFeatures, GroupTypeId = u32>,
{
    TokenRules {
        whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
        paragraphs: ParagraphRules { enabled: true },
        groups: GroupRules {
            enabled: true,
            rules: vec![
                Arc::new(GroupRule { group_type: GT_BRACE, open: "{".into(), close: "}".into() }),
                Arc::new(GroupRule {
                    group_type: GT_BRACKET,
                    open: "[".into(),
                    close: "]".into(),
                }),
            ],
            temporary: Vec::new(),
            expecting_close: None,
        },
        commands: CommandRules {
            enabled: true,
            rules: vec![Arc::new(CommandRule {
                escape_char: '\\',
                name_chars: "abcdefghijklmnopqrstuvwxyz".into(),
            })],
        },
        comments: CommentRules {
            enabled: true,
            rules: vec![Arc::new(CommentRule { start: "%".into() })],
        },
        specials: SpecialsRules { enabled: true },
        forbidden_chars: ForbiddenCharsRules { chars: "".into() },
    }
}

/// The seed state of the scripts here ([`script_rules`], no scopes).
fn script_state<L>() -> Arc<ParsingState<L>>
where
    L: Lang<Features = AllLangFeatures, GroupTypeId = u32, ModeId = (), StateExt = ()>,
{
    Arc::new(ParsingState::new(StateData {
        rules: script_rules(),
        scopes: ScopeStack::new(),
        mode: (),
        ext: (),
    }))
}

/// A primary source.
fn source(content: &str) -> Arc<Source> {
    Arc::new(Source::new(content))
}

/// One segment of a script, as the tests write them.
type Segment<'s> = (&'s Arc<Source>, Range<usize>);

/// A finished scripted parse: the tree with its diagnostics, and how the run ended.
struct Parsed<L: Lang> {
    result: ParseResult<L>,
    stop: StopCause<L>,
}

impl<L: Lang> Parsed<L> {
    /// The root list's children.
    fn roots(&self) -> Vec<NodeRef<'_, L>> {
        self.result.tree.root().children().iter().collect()
    }
}

/// The languages the helpers here drive: a scripted-reader language whose driver is the
/// ready-made one (no command resolution — the tests that need callables live in the
/// preset module).
trait ScriptedTestLang:
    Lang<
        SourceOrigin = Option<String>,
        Tokenization = ScriptedTokenization,
        Driver = StdParseDriver,
        Features = AllLangFeatures,
        GroupTypeId = u32,
        ModeId = (),
        StateExt = (),
        InvocationSyntax = (),
    >
{
}

impl ScriptedTestLang for RelaxedLang {}
impl ScriptedTestLang for TiledScriptedLang {}

/// Drive a [`NodesParser`] over the scripted stream of `segments`, stage the outcome
/// under a root `List` spanning what the reader answers for the parsed stretch, freeze,
/// and run both tree oracles (the all-trees law explicitly, then
/// [`check_tree_invariants`](crate::node::check_tree_invariants), which for a language
/// with `OBEYS_SPAN_TILING = false` is the all-trees law again — test **T10**).
fn run_nodes<L: ScriptedTestLang>(
    segments: &[Segment<'_>],
    recovery: Recovery,
    stop: StopSpec<'_, L>,
) -> Result<Parsed<L>, ParseError> {
    let state = script_state::<L>();
    let (result, stop) = with_scripted_parse(&state, segments, recovery, |cx| {
        let begin = cx.tokens.position_here();
        let mut parser = NodesParser::new(stop);
        let (outcome, delta) = parser.parse(cx)?;
        assert!(delta.is_none(), "a nodes parser reports no pass-through delta");
        let end = cx.tokens.position_here();
        let root_span = cx.source_span_within(&begin, &end)?;
        Ok((outcome.nodes, root_span, outcome.stop))
    })?;
    Ok(Parsed { result, stop })
}

/// The error a run was expected to end in — the counter-tests' unwrap (a finished
/// parse has no `Debug`).
fn expect_error<L: Lang>(run: Result<Parsed<L>, ParseError>) -> ParseError {
    match run {
        Ok(_) => panic!("expected the machinery to reject the stream, but a tree was built"),
        Err(error) => error,
    }
}

/// Run `parse` in a [`ParseContext`] over the scripted stream of `segments`, then stage
/// the nodes it returns under a root `List` with the span it returns, freeze and check.
fn with_scripted_parse<L: ScriptedTestLang, S>(
    state: &Arc<ParsingState<L>>,
    segments: &[Segment<'_>],
    recovery: Recovery,
    parse: impl FnOnce(
        &mut ParseContext<'_, '_, L>,
    ) -> Result<(Vec<BuildId>, SourceSpan<L::SourceOrigin>, S), ParseError>,
) -> Result<(ParseResult<L>, S), ParseError> {
    let mut reader = ScriptedReader::<L>::new(segments, state);
    let mut session = ParserSession::new();
    let driver = StdParseDriver::new(recovery, ());
    let (nodes, root_span, extra) = {
        let mut cx = ParseContext::new(&mut reader, Arc::clone(state), &mut session, &driver);
        parse(&mut cx)?
    };
    let kind = NodeKind::list();
    let ext = L::make_node_ext(
        &kind,
        &root_span,
        state,
        session.builder.staged_children(&nodes),
    )
    .expect("mint the root's node ext");
    let root = session
        .builder
        .add(kind, root_span, Arc::clone(state), nodes, ext, ())
        .expect("stage the root list");
    let result = session.finish(root).expect("freeze the tree");
    crate::node::validate_tree(&result.tree).expect("the all-trees law holds");
    // T10: the span-tiling oracle on every tree built here — for a language with
    // `OBEYS_SPAN_TILING = false` it runs the all-trees law and nothing more.
    crate::node::check_tree_invariants(&result.tree);
    Ok((result, extra))
}

/// The [`ImplementationError`] detail of an aborted parse.
fn implementation_detail(error: &ParseError) -> String {
    error
        .data()
        .downcast_ref::<ImplementationError>()
        .expect("an ImplementationError condition")
        .detail
        .clone()
}

/// A chars node's content **as recorded** (not resolved): what tells `Owned` from
/// `Spanned`.
fn chars_content<'t, L: Lang>(node: &NodeRef<'t, L>) -> &'t TextContent {
    match node.kind() {
        NodeKind::Chars { content } => content,
        other => panic!("expected a chars node, found {other:?}"),
    }
}

/// Assert a chars node records `text` as owned content (the multi-token recording of a
/// language that does not obey span tiling).
fn assert_owned_chars<L: Lang>(node: &NodeRef<'_, L>, text: &str) {
    match chars_content(node) {
        TextContent::Owned(owned) => assert_eq!(&owned[..], text),
        other => panic!("expected owned chars content, found {other:?}"),
    }
    assert_eq!(node.chars(), Some(text));
}

/// Assert a chars node records its content as a span of its own source covering `text`
/// (the single-token recording, where the node's span *is* the fact's span).
fn assert_spanned_chars<L: Lang>(node: &NodeRef<'_, L>, text: &str) {
    match chars_content(node) {
        TextContent::Spanned(_) => {}
        other => panic!("expected span-backed chars content, found {other:?}"),
    }
    assert_eq!(node.chars(), Some(text));
}

/// A minimal source recomposer over the core-complete kinds — the fold used to check
/// that a tree re-emits **as stored** (tests T4/T12).
struct CoreSource;

impl<L: Lang, A> Recomposer<L, A> for CoreSource {
    type State = ();
    type Piece = String;
    type Error = String;

    fn recompose_node(
        &mut self,
        node: NodeRef<'_, L, A>,
        _state: &(),
        _cx: &mut RecomposeContext<'_, L, A>,
    ) -> Result<Recompose<String, ()>, String> {
        core_source_instruction(node).ok_or_else(|| String::from("a callable node"))
    }
}

/// Re-emit a tree from its recorded payloads.
fn recompose<L: Lang>(parsed: &Parsed<L>) -> String {
    TreeRecomposer::new(&mut CoreSource)
        .recompose(&parsed.result.tree, ())
        .expect("the fold runs")
}

// --- T1: a chars run across a seam ----------------------------------------------------

/// The splice of the plan: `A[0..1]` = `"a"`, `B[0..3]` = `"xyz"`, `A[5..7]` = `" b"` —
/// `A`'s `\foo` (bytes 1..5) is the hole the spliced source stands in for.
fn splice_sources() -> (Arc<Source>, Arc<Source>) {
    (source("a\\foo b"), source("xyz"))
}

#[test]
fn a_chars_run_across_a_seam_is_one_node_owning_the_text_it_read() {
    let (a, b) = splice_sources();
    let parsed: Parsed<RelaxedLang> =
        run_nodes(&[(&a, 0..1), (&b, 0..3), (&a, 5..7)], Recovery::Strict, StopSpec::none())
            .expect("the parse runs");

    // One maximal run: the tokens came from two sources, which changes nothing about
    // what a run is.
    let roots = parsed.roots();
    assert_eq!(roots.len(), 1);
    assert_owned_chars(&roots[0], "axyz b");
    // The span is what the reader *describes* for the stretch: `A`, from where the run
    // started to the last offset the stream reached in `A` — the hole included, since
    // the stream came back past it.
    assert!(Arc::ptr_eq(roots[0].span().source(), &a));
    assert_eq!(roots[0].span().range(), 0..7);
    // The span's own content is *not* the node's text: a described span is a coordinate
    // hint, and the recorded content is what the reader answered token by token.
    assert_eq!(roots[0].span().content(), "a\\foo b");
    assert!(parsed.result.diagnostics.is_empty());
    assert_eq!(parsed.stop, StopCause::EndOfInput);
}

#[test]
fn a_tiled_language_rejects_a_chars_run_across_a_seam() {
    // The same script under a language that declares its parse trees span-tiled: the
    // run's two positions delimit no range of one source, which is contract clause 8
    // broken — an implementation error, not a tree.
    let (a, b) = splice_sources();
    let error = expect_error(run_nodes::<TiledScriptedLang>(
        &[(&a, 0..1), (&b, 0..3), (&a, 5..7)],
        Recovery::Strict,
        StopSpec::none(),
    ));
    assert!(
        implementation_detail(&error).contains("do not delimit one range of one source"),
        "unexpected detail: {}",
        implementation_detail(&error)
    );
}

#[test]
fn recomposing_a_run_across_a_seam_emits_the_text_as_stored() {
    // T12: the tree re-emits what the parser recorded — no byte equality with any
    // source is claimed for such a tree, and none is needed.
    let (a, b) = splice_sources();
    let parsed: Parsed<RelaxedLang> =
        run_nodes(&[(&a, 0..1), (&b, 0..3), (&a, 5..7)], Recovery::Strict, StopSpec::none())
            .expect("the parse runs");
    assert_eq!(recompose(&parsed), "axyz b");
}

// --- T1b: a chars run that merely *ends* at a seam -------------------------------------

#[test]
fn a_run_ending_at_a_seam_is_owned_and_a_tiled_language_rejects_it() {
    // `A[0..1]` = `"a"`, `B[0..3]` = `"{b}"`: the run holds one token of `A` and ends
    // where `B` begins. The place past `A`'s last token *is* the place before `B`'s
    // first one — one position, whose coordinate lies in `B` — so even this run has no
    // exact span. A language that does not obey span tiling records the text it read;
    // a tiled one gets the implementation error.
    let a = source("a");
    let b = source("{b}");
    let parsed: Parsed<RelaxedLang> =
        run_nodes(&[(&a, 0..1), (&b, 0..3)], Recovery::Strict, StopSpec::none())
            .expect("the parse runs");
    let roots = parsed.roots();
    assert_eq!(roots.len(), 2);
    assert_owned_chars(&roots[0], "a");
    assert!(Arc::ptr_eq(roots[0].span().source(), &a));
    assert_eq!(roots[0].span().range(), 0..1);
    assert!(roots[1].is_group());
    assert_eq!(roots[1].group_delimiters(), Some(("{", "}")));

    let error = expect_error(run_nodes::<TiledScriptedLang>(
        &[(&a, 0..1), (&b, 0..3)],
        Recovery::Strict,
        StopSpec::none(),
    ));
    assert!(
        implementation_detail(&error).contains("do not delimit one range of one source"),
        "unexpected detail: {}",
        implementation_detail(&error)
    );
}

// --- T2: a group spanning a seam -------------------------------------------------------

#[test]
fn a_group_spanning_a_seam_records_the_delimiter_it_cannot_span() {
    // `A[0..2]` = `"{a"`, `B[0..2]` = `"b}"`: the open delimiter lies in the group
    // node's own source, the close does not.
    let a = source("{a");
    let b = source("b}");
    let parsed: Parsed<RelaxedLang> =
        run_nodes(&[(&a, 0..2), (&b, 0..2)], Recovery::Strict, StopSpec::none())
            .expect("the parse runs");

    let roots = parsed.roots();
    assert_eq!(roots.len(), 1);
    let group = &roots[0];
    assert!(group.is_group());
    // The node's span is what the reader described: `A`, from the open delimiter to
    // where the stream left that source.
    assert!(Arc::ptr_eq(group.span().source(), &a));
    assert_eq!(group.span().range(), 0..2);

    let data = group.group().expect("a group node");
    match &data.open {
        TextContent::Spanned(span) => assert_eq!(span.range(), 0..1),
        other => panic!("the open delimiter lies in the node's own source: {other:?}"),
    }
    match &data.close {
        TextContent::Owned(text) => assert_eq!(&text[..], "}"),
        other => panic!("the close delimiter comes from another source: {other:?}"),
    }
    assert_eq!(group.group_delimiters(), Some(("{", "}")));

    // The interior run crossed the seam, so it owns its text.
    let children: Vec<_> = group.children().iter().collect();
    assert_eq!(children.len(), 1);
    assert_owned_chars(&children[0], "ab");
    assert_eq!(recompose(&parsed), "{ab}");
}

// --- T3: a hole in one source ----------------------------------------------------------

#[test]
fn a_chars_run_across_a_hole_in_one_source_is_one_owned_node() {
    // `A[0..1]` = `"a"`, `A[5..6]` = `"b"`: one source throughout (no seam), with
    // `A`'s `\foo` skipped — as if it had been consumed elsewhere.
    let a = source("a\\foob");
    let parsed: Parsed<RelaxedLang> =
        run_nodes(&[(&a, 0..1), (&a, 5..6)], Recovery::Strict, StopSpec::none())
            .expect("the parse runs");

    let roots = parsed.roots();
    assert_eq!(roots.len(), 1);
    assert_owned_chars(&roots[0], "ab");
    // The described span covers the hole; the recorded text does not.
    assert!(Arc::ptr_eq(roots[0].span().source(), &a));
    assert_eq!(roots[0].span().range(), 0..6);
    assert_eq!(roots[0].span().content(), "a\\foob");

    // A gap is as much a break of contract clause 8 as a change of source is.
    let error = expect_error(run_nodes::<TiledScriptedLang>(
        &[(&a, 0..1), (&a, 5..6)],
        Recovery::Strict,
        StopSpec::none(),
    ));
    assert!(
        implementation_detail(&error).contains("do not delimit one range of one source"),
        "unexpected detail: {}",
        implementation_detail(&error)
    );
}

// --- T5: an un-consumed stop token at a seam ------------------------------------------

#[test]
fn an_unconsumed_stop_token_at_a_seam_is_peeked_again_where_it_stands() {
    // `A[0..2]` = `"ab"`, `B[0..2]` = `"cd"`, stopping at `B`'s first token without
    // consuming it: contract clauses 2 and 7 hold through the seam, so the caller peeks
    // the very token the condition matched, at the very position the run ended at.
    let a = source("ab");
    let b = source("cd");
    let state = script_state::<RelaxedLang>();
    let segments: [Segment<'_>; 2] = [(&a, 0..2), (&b, 0..2)];
    let mut reader = ScriptedReader::<RelaxedLang>::new(&segments, &state);
    let mut session = ParserSession::new();
    let driver = StdParseDriver::new(Recovery::Strict, ());
    let stops_at_c = |token: &_, tokens: &dyn TokenReader<'_, RelaxedLang>| {
        Ok(matches!(tokens.token_kind(token), TokenKind::Char('c')))
    };
    let mut cx = ParseContext::new(&mut reader, Arc::clone(&state), &mut session, &driver);

    let mut parser =
        NodesParser::new(StopSpec::at_token(TokenStopKind::Predicate(&stops_at_c), false));
    let (outcome, _) = parser.parse(&mut cx).expect("the parse runs");

    let StopCause::TokenCondition { span, after } = &outcome.stop else {
        panic!("expected a token-condition stop, got {:?}", outcome.stop);
    };
    assert!(Arc::ptr_eq(span.source(), &b));
    assert_eq!(span.range(), 0..1);

    // The stream stands at the un-consumed token, which is the place the run ended at.
    let here = cx.tokens.position_here();
    let again = cx.tokens.peek(&cx.state).expect("the token is peekable again");
    assert!(matches!(cx.tokens.token_kind(&again), TokenKind::Char('c')));
    assert_eq!(cx.tokens.position_at(&again, TokenEdge::StartBeforePreSpace), here);
    // Nothing of it was staged as content: its pre-space is empty, and its own bytes
    // are still ahead of the stream.
    assert_eq!(
        cx.tokens
            .source_span_between(&again, TokenEdge::StartBeforePreSpace, TokenEdge::Start)
            .content(),
        ""
    );
    assert_eq!(cx.tokens.position_at(&again, TokenEdge::EndPastPostSpace), *after);
}

// --- T6: backtracking across a seam ----------------------------------------------------

/// The `[`…`]` rule of [`script_rules`] — the very `Arc` the seed carries, so that the
/// optional-argument parser's minted rule is the rule the script's tokens name (a fixed
/// script cannot be re-tokenized under the parser's momentary state).
fn bracket_rule<L>(state: &Arc<ParsingState<L>>) -> Arc<GroupRule<L>>
where
    L: Lang<Features = AllLangFeatures, GroupTypeId = u32>,
{
    Arc::clone(
        state
            .rules()
            .groups
            .rules
            .iter()
            .find(|rule| &*rule.open == "[")
            .expect("the bracket rule is in the seed"),
    )
}

#[test]
fn an_optional_argument_probe_matches_across_a_seam() {
    // `A[0..4]` = `"\\cmd"`, `B[0..4]` = `"[x]z"`: the trigger and its optional
    // argument lie in two sources.
    let a = source("\\cmd");
    let b = source("[x]z");
    let state = script_state::<RelaxedLang>();
    let rule = bracket_rule(&state);
    let spec = ArgumentSpec::new(OptionalGroupArgumentParser::new(rule), "opt");
    let (result, ()) = with_scripted_parse::<RelaxedLang, ()>(
        &state,
        &[(&a, 0..4), (&b, 0..4)],
        Recovery::Strict,
        |cx| {
            // Step over the trigger, then probe for the optional argument.
            let state = Arc::clone(&cx.state);
            let trigger = cx.probe_token(&state)?.expect("the trigger token");
            assert!(matches!(
                cx.tokens.token_kind(&trigger),
                TokenKind::Command { name: "cmd", .. }
            ));
            cx.tokens.move_to(&trigger, TokenEdge::EndPastPostSpace);
            let begin = cx.tokens.position_here();
            let parsed = spec
                .parser
                .parse_argument(cx, &spec)?
                .expect("the optional argument is present");
            let end = cx.tokens.position_here();
            let span = cx.source_span_within(&begin, &end)?;
            Ok((parsed.nodes, span, ()))
        },
    )
    .expect("the parse runs");
    let parsed = Parsed { result, stop: StopCause::EndOfInput };

    let roots = parsed.roots();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].group_delimiters(), Some(("[", "]")));
    let content: Vec<_> = roots[0].children().iter().collect();
    assert_eq!(content.len(), 1);
    assert_owned_chars(&content[0], "x");
    assert_eq!(recompose(&parsed), "[x]");
}

#[test]
fn an_optional_argument_probe_that_fails_rewinds_across_a_seam() {
    // `A[0..6]` = `"\\cmd%c"`, `B[0..1]` = `"y"`: the probe reads `A`'s comment as
    // leading noise, finds `B`'s `y` where an opening `[` would have to be, and rewinds
    // — back across the seam, into the source the stream has already left (contract
    // clause 3). The comment is peekable again exactly where it was.
    let a = source("\\cmd%c");
    let b = source("y");
    let state = script_state::<RelaxedLang>();
    let rule = bracket_rule(&state);
    let spec = ArgumentSpec::new(OptionalGroupArgumentParser::new(rule), "opt");

    let segments: [Segment<'_>; 2] = [(&a, 0..6), (&b, 0..1)];
    let mut reader = ScriptedReader::<RelaxedLang>::new(&segments, &state);
    let mut session = ParserSession::new();
    let driver = StdParseDriver::new(Recovery::Strict, ());
    let mut cx = ParseContext::new(&mut reader, Arc::clone(&state), &mut session, &driver);

    let trigger = cx.tokens.peek(&cx.state).expect("the trigger");
    cx.tokens.move_to(&trigger, TokenEdge::EndPastPostSpace);
    let before = cx.tokens.position_here();

    let absent = spec.parser.parse_argument(&mut cx, &spec).expect("the probe runs");
    assert!(absent.is_none(), "no bracket follows, so the argument is absent");

    assert_eq!(cx.tokens.position_here(), before, "the probe rewound to where it started");
    let again = cx.tokens.peek(&cx.state).expect("the comment is peekable again");
    assert!(matches!(
        cx.tokens.token_kind(&again),
        TokenKind::Comment { start_delim: "%", content: "c" }
    ));
    assert!(Arc::ptr_eq(cx.tokens.source_span_of(&again).source(), &a));
}

// --- T7: a comment and a paragraph break in the spliced source ------------------------

#[test]
fn a_comment_and_a_paragraph_break_in_another_source_become_nodes() {
    // `A[0..1]` = `"a"`, `B[0..8]` = `"%note\n\nb"`.
    let a = source("a");
    let b = source("%note\n\nb");
    let parsed: Parsed<RelaxedLang> =
        run_nodes(&[(&a, 0..1), (&b, 0..8)], Recovery::Strict, StopSpec::none())
            .expect("the parse runs");

    let roots = parsed.roots();
    assert_eq!(roots.len(), 4, "{:?}", roots.iter().map(|n| n.summary()).collect::<Vec<_>>());

    // The run that ended at the seam owns its text.
    assert_owned_chars(&roots[0], "a");
    assert!(Arc::ptr_eq(roots[0].span().source(), &a));

    // The comment's three sub-spans are reader answers about one token, recorded
    // through the node-data rule: this token lies wholly in `B`, which is also the
    // node's source, so all three stay span-backed.
    let comment = roots[1].comment().expect("a comment node");
    assert!(Arc::ptr_eq(roots[1].span().source(), &b));
    assert_eq!(roots[1].span().range(), 0..5);
    for part in [&comment.start, &comment.content, &comment.post_space] {
        assert!(
            matches!(part, TextContent::Spanned(_)),
            "the comment's parts lie in the node's own source: {part:?}"
        );
    }
    assert_eq!(comment.start.resolve(roots[1].source()), "%");
    assert_eq!(comment.content.resolve(roots[1].source()), "note");
    assert_eq!(comment.post_space.resolve(roots[1].source()), "");

    // The paragraph break is its own node, spanning exactly the token it was made
    // from — the node's span *is* the fact's span, so it stays span-backed too.
    assert_spanned_chars(&roots[2], "\n\n");
    assert_eq!(roots[2].span().range(), 5..7);

    // The run after it never crossed a seam, but the language's declaration is what
    // decides the recording, not the run's luck.
    assert_owned_chars(&roots[3], "b");
    assert_eq!(roots[3].span().range(), 7..8);
    assert_eq!(recompose(&parsed), "a%note\n\nb");
}

// --- T9: a reader that breaks the seam position ---------------------------------------

/// Run the two-source script `"a"` + `"b"` through
/// [`ScriptedReader::broken_at_seams`], which reports the place past `A`'s last token
/// and the place before `B`'s first one as two different positions, and return the
/// error the machinery reports.
fn parse_with_broken_seams<L: ScriptedTestLang>() -> ParseError {
    let a = source("a");
    let b = source("b");
    let state = script_state::<L>();
    let segments: [Segment<'_>; 2] = [(&a, 0..1), (&b, 0..1)];
    let mut reader = ScriptedReader::<L>::broken_at_seams(&segments, &state);
    let mut session = ParserSession::new();
    let driver = StdParseDriver::new(Recovery::Strict, ());
    let mut cx = ParseContext::new(&mut reader, Arc::clone(&state), &mut session, &driver);
    let mut parser = NodesParser::new(StopSpec::<L>::none());
    parser.parse(&mut cx).expect_err("the broken reader is rejected")
}

#[test]
fn a_token_not_starting_where_the_stream_stood_is_an_implementation_error() {
    // Contract clause 7's corollary — two consecutive tokens meet at one position —
    // holds for every reader, seams included, and the content loop checks it whatever
    // the language declares about span tiling.
    for detail in [
        implementation_detail(&parse_with_broken_seams::<RelaxedLang>()),
        implementation_detail(&parse_with_broken_seams::<TiledScriptedLang>()),
    ] {
        assert!(
            detail.contains("is not the position the stream stood at when the token was peeked"),
            "unexpected detail: {detail}"
        );
        assert!(
            detail.contains("violates the `TokenReader` contract"),
            "unexpected detail: {detail}"
        );
    }
}

// --- T11: diagnostics inside a synthesized source -------------------------------------

#[test]
fn a_diagnostic_in_a_synthesized_source_renders_the_provenance_chain() {
    // `A[0..1]` = `"x"`, then a source minted as `Synthesized { description,
    // triggered_at }` — what a reader that expands a macro as it reads is asked to do,
    // so that a condition detected inside the expansion carries the chain back to where
    // the expansion was triggered. The condition here is a real one: the group opened
    // in the synthesized source is never closed.
    let a = source("x\\expandme");
    let expansion: Arc<Source> = Arc::new(Source::synthesized(
        "{y",
        "macro expansion",
        SourceSpan::new(&a, 1..10),
    ));
    let parsed: Parsed<RelaxedLang> = run_nodes(
        &[(&a, 0..1), (&expansion, 0..2)],
        Recovery::Tolerant,
        StopSpec::none(),
    )
    .expect("tolerant parsing keeps going");

    assert_eq!(parsed.result.diagnostics.len(), 1);
    let report = parsed.result.diagnostics.render_all();
    assert!(report.contains("unclosed group"), "unexpected report:\n{report}");
    assert!(
        report.contains("synthesized from") && report.contains("macro expansion"),
        "the provenance chain is missing from the report:\n{report}"
    );
}

// --- T13: verbatim content that begins at a seam --------------------------------------

/// The token rules a raw-content script is tokenized under: whitespace on, every
/// delimiter recognizer off, and `}` installed as the **expected close** — which is
/// exactly the shape [`verbatim_state_delta`](super::verbatim_state_delta) derives, so
/// a fixed script serves the verbatim parser faithfully. Whitespace stays on, which the
/// verbatim state itself turns off: that is what lets the terminator arrive with
/// pre-space, the one arm of the raw-content loop a scanning reader can never produce.
fn raw_script_state<L>() -> Arc<ParsingState<L>>
where
    L: Lang<Features = AllLangFeatures, GroupTypeId = u32, ModeId = (), StateExt = ()>,
{
    let mut rules = script_rules::<L>();
    rules.groups.enabled = false;
    rules.groups.rules = Vec::new();
    rules.commands.enabled = false;
    rules.comments.enabled = false;
    rules.paragraphs.enabled = false;
    rules.groups.expecting_close = Some(Arc::new(GroupRule {
        group_type: GT_BRACE,
        open: "{".into(),
        close: "}".into(),
    }));
    Arc::new(ParsingState::new(StateData {
        rules,
        scopes: ScopeStack::new(),
        mode: (),
        ext: (),
    }))
}

#[test]
fn verbatim_content_starting_at_a_seam_is_staged_as_the_text_it_read() {
    // `A[0..1]` = `"{"`, `B[0..2]` = `"ab"`, `A[4..7]` = `"  }"`: the raw content
    // begins exactly at the seam into `B` and ends with the whitespace that arrives as
    // the terminator's pre-space — content, by the raw-content loop's rule. The
    // described span (`B[0..2]` = `"ab"`) therefore does not hold the content's text
    // (`"ab  "`), which is why the text decides both what is recorded and whether there
    // is anything to record at all.
    let a = source("{zzz  }");
    let b = source("ab");
    let state = raw_script_state::<RelaxedLang>();
    let segments: [Segment<'_>; 3] = [(&a, 0..1), (&b, 0..2), (&a, 4..7)];
    let mut reader = ScriptedReader::<RelaxedLang>::new(&segments, &state);
    let mut session = ParserSession::new();
    let driver = StdParseDriver::new(Recovery::Strict, ());

    let spec = ArgumentSpec::new(VerbatimArgumentParser::new(GT_BRACE), "verb");
    let nodes = {
        let mut cx = ParseContext::new(&mut reader, Arc::clone(&state), &mut session, &driver);
        spec.parser
            .parse_argument(&mut cx, &spec)
            .expect("the verbatim argument parses")
            .expect("the argument is present")
            .nodes
    };

    let root_span = SourceSpan::new(&a, 0..7);
    let kind = NodeKind::list();
    let root = session
        .builder
        .add(kind, root_span, Arc::clone(&state), nodes, (), ())
        .expect("stage the root list");
    let result = session.finish(root).expect("freeze the tree");
    crate::node::validate_tree(&result.tree).expect("the all-trees law holds");
    crate::node::check_tree_invariants(&result.tree);

    let group = result.tree.root().child(0).expect("the verbatim group");
    assert_eq!(group.group_delimiters(), Some(("{", "}")));
    assert!(Arc::ptr_eq(group.span().source(), &a));
    assert_eq!(group.span().range(), 0..7);

    let content: Vec<_> = group.children().iter().collect();
    assert_eq!(content.len(), 1);
    // The accumulated text, including the terminator's pre-space …
    assert_owned_chars(&content[0], "ab  ");
    // … while the span the reader described for the stretch says something else.
    assert!(Arc::ptr_eq(content[0].span().source(), &b));
    assert_eq!(content[0].span().range(), 0..2);
    assert_eq!(content[0].span().content(), "ab");
}
