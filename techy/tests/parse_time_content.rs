//! Reading an argument's content **while the parse is still running**, through the
//! public API only.
//!
//! Two custom callable specs stand in for what a crate-external definition author
//! writes. Both take over their own invocation parse and read their argument back
//! before staging it:
//!
//! - `\include` is `\input`-shaped: it reads its argument as a source reference with
//!   [`ParseContext::argument_content_as_tree`] plus
//!   [`techy::extract::content_as_chars`];
//! - `\keys` splits its argument at `","` with
//!   [`techy::extract::split_at_chars_drop_annotations`] and records how many segments
//!   it found.
//!
//! Both record what they read into a shared log the assertions then read.

use std::sync::{Arc, Mutex};

use techy::core::constructs::{
    parse_declared_arguments, ConstructParser, ConstructParserResult, Invocation, ParseContext,
};
use techy::core::node::{validate_tree, BuildId, NodeBuildError, ParsedArguments, ParsedSlots};
use techy::core::specs::{ArgumentSpec, CallableSpec, Package};
use techy::core::token::TokenEdge;
use techy::core::{FrameRole, Language, ParseResult, ParsingState, ParsingStateDelta};
use techy::error::{HookFailed, ParseError, Recovery};
use techy::source::SourceSpan;
use techy::extract::{content_as_chars, split_at_chars_drop_annotations};
use techy::latexlike::{argument_specs, CallableType, Latexlike, LatexlikeDriver, MacroSpec};
use techy::serialize::SerializableObject;

/// What the two specs record, in invocation order.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Read {
    /// `\include`'s argument, read as one flat string.
    Reference(String),
    /// `\include`'s argument was not provided at all.
    NoReference,
    /// `\keys`'s argument, split at `","`: the segments, flattened.
    Keys(Vec<String>),
}

type Log = Arc<Mutex<Vec<Read>>>;

// --- the two specs ------------------------------------------------------------------

/// What a spec's invocation parser does with the argument it read.
type Reader = fn(&mut ParseContext<'_, '_, Latexlike>, &Log, Option<Tree>) -> Result<(), Failure>;

type Tree = techy::core::node::NodeTree<Latexlike, Option<BuildId>>;
type Failure = ParseError<Option<String>>;

/// A macro with one mandatory argument, whose invocation parser reads that argument's
/// content at parse time and hands it to `read`.
#[derive(Debug)]
struct ReadingSpec {
    arguments: Vec<Arc<ArgumentSpec<Latexlike>>>,
    read: Reader,
    log: Log,
}

impl SerializableObject<Latexlike> for ReadingSpec {}

impl CallableSpec<Latexlike> for ReadingSpec {
    fn arguments(&self) -> &[Arc<ArgumentSpec<Latexlike>>] {
        &self.arguments
    }

    fn stack_frame_title(&self, _role: FrameRole, name: &str) -> String {
        format!("macro \\{name}")
    }

    fn make_invocation_parser<'a>(
        &'a self,
        invocation: Invocation<'a, Latexlike>,
    ) -> Result<Box<dyn ConstructParser<Latexlike, Output = BuildId> + 'a>, Failure> {
        Ok(Box::new(ReadingParser { invocation, read: self.read, log: Arc::clone(&self.log) }))
    }
}

struct ReadingParser<'a> {
    invocation: Invocation<'a, Latexlike>,
    read: Reader,
    log: Log,
}

impl ConstructParser<Latexlike> for ReadingParser<'_> {
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, Latexlike>,
    ) -> ConstructParserResult<Latexlike, (BuildId, Option<Box<ParsingStateDelta<Latexlike>>>)>
    {
        let token = self.invocation.token;
        let name = cx.tokens.source_span_between(token, TokenEdge::Start, TokenEdge::End);

        // The declared arguments, exactly as the standard invocation parser reads them.
        let (children, arguments) = parse_declared_arguments(cx, self.invocation.spec, &name)?;

        // The argument's content, copied into a tree of its own so that the reading
        // helpers of `techy::extract` apply here, mid-parse. A contract violation in
        // this parser would come back as an error, never as a panic.
        let argument = arguments.first().expect("one declared argument");
        let tree = cx
            .argument_content_as_tree(argument, &children)
            .map_err(|error| lift(cx, error, cx.here()))?;
        (self.read)(cx, &self.log, tree)?;

        let id = cx.stage_invocation(
            &self.invocation,
            ParsedArguments::from(arguments),
            ParsedSlots::empty(),
            children,
            None,
        )?;
        Ok((id, None))
    }
}

/// The two-arm lift a failed read documents: the root ext mint's own reported failure
/// is an operational failure in consumer-supplied hook code, and every other variant is
/// a contract violation by this parser. Both abort under any recovery policy.
fn lift(
    cx: &ParseContext<'_, '_, Latexlike>,
    error: NodeBuildError,
    span: SourceSpan<Option<String>>,
) -> Failure {
    match error {
        NodeBuildError::ExtMintFailed { detail } => {
            ParseError::new(HookFailed::new(detail, None), span)
                .with_frames(cx.session.snapshot_frames())
        }
        other => cx.implementation_error(other, span),
    }
}

/// `\include`: the `\input` shape — the argument names one source.
fn read_reference(
    cx: &mut ParseContext<'_, '_, Latexlike>,
    log: &Log,
    tree: Option<Tree>,
) -> Result<(), Failure> {
    let read = match &tree {
        Some(tree) => {
            let text = content_as_chars(tree.root().children())
                .map_err(|error| cx.implementation_error(error, cx.here()))?;
            Read::Reference(text.into_owned())
        }
        // The argument was not provided; the argument parser diagnosed that already.
        None => Read::NoReference,
    };
    log.lock().unwrap().push(read);
    Ok(())
}

/// `\keys`: the argument is a comma-separated list, split at parse time.
fn read_keys(
    cx: &mut ParseContext<'_, '_, Latexlike>,
    log: &Log,
    tree: Option<Tree>,
) -> Result<(), Failure> {
    let Some(tree) = tree else {
        log.lock().unwrap().push(Read::Keys(Vec::new()));
        return Ok(());
    };
    let split = split_at_chars_drop_annotations(tree.root().children(), ",")
        .map_err(|error| cx.implementation_error(error, cx.here()))?;
    let mut keys = Vec::new();
    for segment in split.segments() {
        let text = content_as_chars(segment)
            .map_err(|error| cx.implementation_error(error, cx.here()))?;
        keys.push(text.into_owned());
    }
    log.lock().unwrap().push(Read::Keys(keys));
    Ok(())
}

// --- the language -------------------------------------------------------------------

fn language(recovery: Recovery) -> (Language<Latexlike>, Log) {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let mut package = Package::new("parse-time-content");
    package.insert(
        CallableType::Macro,
        "include",
        Arc::new(ReadingSpec {
            arguments: argument_specs(["m"]).unwrap(),
            read: read_reference,
            log: Arc::clone(&log),
        }),
    );
    package.insert(
        CallableType::Macro,
        "keys",
        Arc::new(ReadingSpec {
            arguments: argument_specs(["m"]).unwrap(),
            read: read_keys,
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
    );
    (language, log)
}

fn parse(input: &str) -> (ParseResult<Latexlike>, Vec<Read>) {
    let (language, log) = language(Recovery::Strict);
    let result = language.parse(input).expect("a clean parse");
    assert!(validate_tree(&result.tree).is_ok(), "{:?}", validate_tree(&result.tree));
    assert!(result.diagnostics.is_empty(), "unexpected diagnostics: {:?}", result.diagnostics);
    let reads = std::mem::take(&mut *log.lock().unwrap());
    (result, reads)
}

// --- the tests ----------------------------------------------------------------------

#[test]
fn an_argument_that_was_not_provided_reads_as_none() {
    // A tolerant parse of `\include` with nothing after it: the argument parser
    // diagnoses the missing mandatory argument, and the reader answers `None` rather
    // than an empty reference.
    let (language, log) = language(Recovery::Tolerant);
    let result = language.parse(r"\include").expect("a tolerant parse recovers");
    assert_eq!(result.diagnostics.len(), 1);
    let reads = std::mem::take(&mut *log.lock().unwrap());
    assert_eq!(reads, [Read::NoReference]);
}

#[test]
fn an_input_like_spec_reads_its_reference_at_parse_time() {
    let (result, reads) = parse(r"A\include{chapters/one.tex}B");
    assert_eq!(reads, [Read::Reference("chapters/one.tex".into())]);

    // The document parsed normally around the reading.
    let root = result.tree.root();
    assert_eq!(root.child_count(), 3);
    assert_eq!(root.child(0).unwrap().chars(), Some("A"));
    let include = root.child(1).unwrap();
    assert_eq!(include.macro_name(), Some("include"));
    assert_eq!(include.span_content(), r"\include{chapters/one.tex}");
    assert_eq!(root.child(2).unwrap().chars(), Some("B"));
}

#[test]
fn the_reference_read_is_the_flattened_argument_content() {
    // `content_as_chars` descends into groups and skips comments, so a protective
    // group and a comment both flatten away — this spec's rule, chosen by reading the
    // copied tree rather than the strict `content_as_plain_chars`.
    let (_, reads) = parse("\\include{{chap}% a note\n.tex}");
    assert_eq!(reads, [Read::Reference("chap.tex".into())]);
}

#[test]
fn each_invocation_reads_its_own_argument() {
    let (_, reads) = parse(r"\include{one.tex} and \include{two.tex}");
    assert_eq!(
        reads,
        [Read::Reference("one.tex".into()), Read::Reference("two.tex".into())]
    );
}

#[test]
fn a_reading_spec_works_inside_another_callables_argument() {
    // `\keys` reads its own argument while `\emph`'s argument parse — and the whole
    // enclosing document parse — is still in progress. Reading copies; it stages
    // nothing and claims nothing, so the enclosing parse is unaffected.
    let (result, reads) = parse(r"x\emph{\keys{a,b}}y");
    assert_eq!(reads, [Read::Keys(vec!["a".into(), "b".into()])]);

    let emph = result.tree.root().child(1).unwrap();
    assert_eq!(emph.macro_name(), Some("emph"));
    let content = emph.argument_content_nodes(0).unwrap();
    assert_eq!(content.len(), 1);
    assert_eq!(content.first().unwrap().macro_name(), Some("keys"));
    assert_eq!(result.tree.root().child(2).unwrap().chars(), Some("y"));
}

#[test]
fn a_spec_splits_its_argument_at_a_separator_at_parse_time() {
    let (_, reads) = parse(r"\keys{alpha, beta ,gamma}");
    assert_eq!(
        reads,
        [Read::Keys(vec!["alpha".into(), " beta ".into(), "gamma".into()])]
    );

    // Grouped content protects its interior, exactly as over a finished tree.
    let (_, reads) = parse(r"\keys{a,b{x,y},c}");
    assert_eq!(reads, [Read::Keys(vec!["a".into(), "bx,y".into(), "c".into()])]);

    // Empty content splits into nothing.
    let (_, reads) = parse(r"\keys{}");
    assert_eq!(reads, [Read::Keys(Vec::new())]);
}
