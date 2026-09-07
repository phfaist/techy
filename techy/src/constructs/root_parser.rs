//! [`RootNodesParser`]: the standard root parser — the construct parser that the parse
//! entry point runs directly, at the root of the descent hierarchy, over the whole
//! source.

use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::node::{BuildId, NodeKind};
use crate::source::SourceSpan;
use crate::state::{FeaturePresence, Lang, LangFeatures, ParsingStateDelta};

use super::{
    ChildStateSpec, ConstructParser, ConstructParserResult, FromInvocation, ParseContext,
    StopCause, StopSpec, StrayGroupClose,
};

/// The standard **root parser**: the content loop the parse entry point runs over the
/// whole source, staging the tree's root `List` node.
///
/// Read the name as *root — nodes parser*: "root" is the parser's place in the
/// descent hierarchy (the parser the entry point runs directly, above every descent),
/// and "nodes parser" is what it is — the [`NodesParser`](super::NodesParser)-shaped
/// content loop. It does not parse "root nodes".
///
/// This is the parser [`ParseDriver::make_root_parser`] supplies by default, so it is
/// what [`Language::parse`] and a plain [`ParseSetup::parse`] run. A language whose
/// parses need a different root shape overrides that factory; one parse that does
/// passes its own root parser through [`ParseSetup::with_root_parser`]. Code driving
/// construct parsers over a hand-built [`ParseContext`] can run this parser directly
/// to get the standard root parse.
///
/// # What it does
///
/// Starting from the context's state (the parse's initial state) it runs the content
/// loop through [`ParseContext::parse_nodes`] under [`StopSpec::none`] — through the
/// driver's [`make_nodes_parser`](crate::core::ParseDriver::make_nodes_parser)
/// factory, like every other content descent, so an override there applies to the
/// root run too — and reacts to how each run ends:
///
/// - **End of input**: stage the root `List` over the entire source
///   ([`SourceSpan::entire`]), recording the entry state, with the run's nodes as
///   children; that `List`'s id is the output.
/// - **A stray group close** — nobody's to claim: diagnosed as [`StrayGroupClose`]
///   through the recovery entry point ([`ParseContext::recover`]). A tolerant parse
///   consumes the delimiter, stages it as a `Chars` node (the markup-in-chars recovery
///   artifact, so the root's children keep tiling the source across the skip), and
///   resumes; a strict parse aborts. Diagnosis and resume both run under the state
///   the loop had reached at the close (the run's exit state,
///   [`NodesOutcome::state`](super::NodesOutcome::state)): the reported delimiter is
///   the one the loop's tokenization matched, and sibling-level state changes from
///   before the skip — a `\newcommand`-style definition, a group-rule change — stay in
///   effect across it, exactly as if the close had not been there.
/// - **A token or node stop condition**: none was set, so this is a nodes-parser
///   contract violation — an [`ImplementationError`](super::ImplementationError),
///   aborting under any recovery policy.
///
/// The context's [`state`](ParseContext::state) is left at the loop's exit state; the
/// pass-through delta is always `None` (nothing encloses the root).
///
/// # Contract for any root parser
///
/// A root parser — this one or a replacement — is run **at the top, not as a
/// descent**: the entry point calls [`ConstructParser::parse`] directly rather than
/// through [`ParseContext::parse_construct`], so no descent-guard level and no
/// traceback frame cover it. Its output is the tree's root ([`BuildId`]), which the
/// entry point hands to [`ParserSession::finish`](crate::core::ParserSession::finish);
/// its pass-through delta is discarded. On entry, `cx.state` is the parse's initial
/// state and the reader stands at the start of the source.
///
/// [`ParseDriver::make_root_parser`]: crate::core::ParseDriver::make_root_parser
/// [`Language::parse`]: crate::core::Language::parse
/// [`ParseSetup::parse`]: crate::core::ParseSetup::parse
/// [`ParseSetup::with_root_parser`]: crate::core::ParseSetup::with_root_parser
#[derive(Debug, Default, Clone, Copy)]
pub struct RootNodesParser;

impl RootNodesParser {
    /// The standard root parser (it carries no configuration).
    pub fn new() -> RootNodesParser {
        RootNodesParser
    }
}

impl<L: Lang> ConstructParser<L> for RootNodesParser
where
    L::InvocationSyntax: FromInvocation<L>,
{
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (BuildId, Option<Box<ParsingStateDelta<L>>>)> {
        // The entry state is what the root `List` records; the whole source is what
        // it spans — both read off the context at entry (the reader's source is the
        // parse's source).
        let entry_state = Arc::clone(&cx.state);
        let source = Arc::clone(cx.here().source());
        let mut nodes = Vec::new();
        loop {
            // The root descent routes through the driver's factory like every other
            // descent site (the uniform-routing contract). A pass-through delta has no
            // applicable target at the root and is discarded.
            let (outcome, _delta) = cx.parse_nodes(
                Arc::clone(&cx.state),
                StopSpec::none(),
                ChildStateSpec::inherit(),
            )?;
            nodes.extend(outcome.nodes);
            // Thread the segment's exit state: the root context's ambient state
            // advances with the content, so the recover funnel below and any resume
            // run under the state the loop actually reached — resuming from the entry
            // state would roll back sibling after-effects (`\newcommand` definitions)
            // across a tolerant skip.
            cx.state = outcome.state;
            match outcome.stop {
                StopCause::EndOfInput => break,
                StopCause::UnexpectedGroupClose { span, after } => {
                    // Impossible under a language that declares groups absent: a
                    // group close cannot be tokenized, so reaching this arm means the
                    // token source violated its contract (`TokenReader` docs) — an
                    // implementation bug aborts under any policy, never a panic.
                    if !<L::Features as LangFeatures>::Groups::PRESENT {
                        return Err(cx.implementation_error(
                            "a stray group close surfaced at the root although the \
                             language declares the groups feature absent \
                             (token-source contract violation)",
                            span,
                        ));
                    }
                    // Diagnose-and-skip at the root: the loop left the close
                    // unconsumed at `span.start`, and the span is the delimiter
                    // exactly as matched (`StopCause`'s contract) — sliced, not
                    // re-peeked: a re-read under any state but the loop's own could
                    // tokenize different bytes.
                    let delim = span.content().to_string();
                    cx.recover(StrayGroupClose { delim }, span.clone())?;
                    cx.tokens.move_to_position(&after);
                    // Stage the consumed delimiter as a chars node (the
                    // markup-in-chars recovery artifact): the root partition stays
                    // exact across the skip.
                    let id = cx
                        .stage_node(
                            NodeKind::chars(span.span()),
                            span.clone(),
                            Arc::clone(&cx.state),
                            Vec::new(),
                        )
                        .map_err(|error| cx.staging_error(error, span))?;
                    nodes.push(id);
                }
                StopCause::TokenCondition { span, .. } => {
                    return Err(cx.implementation_error(
                        "the root content loop stopped on a token condition none was \
                         set (nodes-parser contract violation)",
                        span,
                    ));
                }
                StopCause::NodeCondition => {
                    return Err(cx.implementation_error(
                        "the root content loop stopped on a node condition none was \
                         set (nodes-parser contract violation)",
                        cx.here(),
                    ));
                }
            }
        }
        let root = cx
            .stage_node(NodeKind::list(), SourceSpan::entire(&source), entry_state, nodes)
            .map_err(|error| {
                let at = SourceSpan::at(&SourceSpan::entire(&source).start_pos());
                cx.staging_error(error, at)
            })?;
        Ok((root, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{ParserSession, StdParseDriver};
    use crate::error::Recovery;
    use crate::node::check_tree_invariants;
    use crate::source::Source;
    use crate::state::{ParsingState, TrivialLang};
    use crate::token::Tokenization;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl TrivialLang for PlainLang {}

    #[test]
    fn driven_by_hand_it_stages_the_root_list_over_the_whole_source() {
        // The advanced path: a hand-built context, the standard root parser run
        // directly, the session frozen around its output.
        let driver = StdParseDriver::new(Recovery::Strict, ());
        let state = Arc::new(ParsingState::<PlainLang>::lang_initial().expect("seed state"));
        let source = Arc::new(Source::new("hello"));
        let mut reader = <crate::token::StdTokenization as Tokenization<PlainLang>>::make_token_reader(&source);
        let mut session = ParserSession::new();
        let mut cx = ParseContext::new(&mut *reader, Arc::clone(&state), &mut session, &driver);
        let (root, delta) = RootNodesParser::new().parse(&mut cx).unwrap();
        assert!(delta.is_none(), "the root has no caller to apply a delta");
        let result = session.finish(root).unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(result.tree.root().span().range(), 0..5);
        assert!(Arc::ptr_eq(result.tree.root().parsing_state(), &state));
        assert_eq!(result.tree.root().child(0).unwrap().chars(), Some("hello"));
    }
}
