//! [`GroupParser`]: parses one delimited group — interior content up to and including
//! the close delimiter — into a `Group` node (pylatexenc's `LatexDelimitedGroupParser`).
//!
//! # Contract
//!
//! Constructed with the opening delimiter's span and its resolved
//! [`GroupRule`] — the two facts the [`GroupOpen`](crate::token::TokenKind::GroupOpen)
//! trigger token carries. The **caller consumes the trigger token** before running the
//! parser (the dispatch-loop arm that peeked it, under the state that tokenized it —
//! the same at-match-time atomicity rule as the stop-condition consume flag; it also
//! keeps this parser free of `'s`-bound token storage, which the uniform
//! [`ConstructParser::parse`] signature could not tie to the context's reader). The
//! token's pre-space is likewise the caller's (housed as sibling content).
//!
//! `cx.state` is the interior's **base** state (the caller resolves any
//! [`ChildStateSpec`](super::ChildStateSpec) policy first); the parser derives the
//! actual interior state from it — base + `expecting_group_close` from the opening
//! rule, via the session's memoized
//! [`group_interior_state`](crate::engine::ParserSession::group_interior_state) — so the
//! close delimiter is guaranteed recognizable regardless of the base's delimiter table
//! (the `$…$` case: the ambiguous `$` closes only through the expected-close rule). The
//! interior state is scoped structurally: `cx.state` is swapped for the recursion and
//! restored after (the group has no after-effect — the returned delta is always `None`).
//!
//! # Matching and recovery
//!
//! The interior [`NodesParser`] stops at the exact `(group_type, close)` pairing the
//! group opened with, **consuming** the close at match time (the consume flag's
//! atomicity guarantee), and the `Group` node records both delimiters span-backed
//! ([`GroupData`]). Tolerant recovery, at this detection site:
//!
//! - *End of input inside the group*: diagnostic (at the open delimiter) + close the
//!   group with an **empty** `close` ([`GroupData`]'s documented recovery value).
//! - *Unexpected group close inside* (a `]` under a `{`, a re-classed `}`): diagnostic +
//!   close the group **without consuming** the stray token — the same unwinding rule as
//!   environment-terminator mismatch: every level either consumes the token or
//!   unwinds out of its own frame, and the stray close eventually reaches a level that
//!   claims it (or the root, which diagnoses and skips).
//!
//! Under [`Recovery::Strict`](crate::error::Recovery) both conditions abort instead.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;

use crate::engine::{Frame, FrameTitle};
use crate::error::{DiagnosticInfo, ToDiagnosticValue};
use crate::node::{BuildId, GroupData, NodeKind};
use crate::source::{SourceSpan, Span, TextContent};
use crate::state::{Lang, ParsingStateDelta};
use crate::token::GroupRule;

use super::child_state::ChildStateSpec;
use super::nodes_parser::{StopCause, StopSpec, TokenStopKind};
use super::{ConstructParser, ConstructParserResult, FromInvocation, ParseContext};

/// Condition: a delimited group was never closed with its expected delimiter — detected
/// by [`GroupParser`], which defines the condition next to its detection site.
///
/// Tolerant recovery, per [`found`](UnclosedGroup::found) situation: at end of input,
/// the group closes with an empty recorded `close`
/// ([`GroupData`](crate::node::GroupData)'s documented recovery value); on a close
/// delimiter of a different pairing, the group closes **without consuming** the stray
/// token, which is left for an enclosing level to claim (or for the root, which
/// diagnoses and skips it). Strict parses abort instead.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.groups.unclosed-group")]
pub struct UnclosedGroup {
    /// The close delimiter the group expected (as written, e.g. `}`).
    pub expected_close: String,
    /// What blocked the close instead.
    pub found: UnclosedGroupFound,
}

/// What an [`UnclosedGroup`] ran into instead of its close delimiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToDiagnosticValue)]
#[non_exhaustive]
pub enum UnclosedGroupFound {
    /// The input ended inside the group.
    EndOfInput,
    /// A close delimiter belonging to a different pairing appeared (the group unwinds,
    /// leaving the stray token for an enclosing level to claim).
    StrayClose,
}

// Hand-written wording: the message varies by what blocked the close (a match, which
// the message format string cannot express). The two recovery situations read
// differently on purpose (July 2026, Action 06): EOF points at the open delimiter of a
// group nothing will ever close; a stray close points at the delimiter that broke the
// pairing.
impl fmt::Display for UnclosedGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.found {
            UnclosedGroupFound::EndOfInput => write!(
                f,
                "unclosed group: expected ‘{}’ before end of input",
                self.expected_close
            ),
            UnclosedGroupFound::StrayClose => {
                write!(f, "mismatched group close: expected ‘{}’", self.expected_close)
            }
        }
    }
}

/// The group construct parser: a tier-2 temporary, constructed per group descent
/// from the opening token's facts (its span and resolved
/// [`GroupRule`](crate::token::GroupRule)). The caller has already consumed the
/// opening token; `cx.state` is the interior's **base** state, from which the parser
/// derives the actual interior state (base plus the expected close delimiter from
/// the opening rule), scoped structurally over the descent. Recovery for a group
/// that never closes is documented on [`UnclosedGroup`].
pub struct GroupParser<'p, L: Lang> {
    /// The opening delimiter's span (the consumed `GroupOpen` token's span).
    open_span: Span,
    /// The opening token's resolved rule: the close spelling and group class of the
    /// pairing to match.
    rule: Arc<GroupRule<L>>,
    /// Descent-state policy handed to the interior [`NodesParser`]; defaults to
    /// inherit-everywhere (policies are one level deep, so a
    /// plain arm-driven descent never propagates one). A parser that scopes the group's
    /// interior state sets it per use — e.g. the chars-except-groups argument pattern,
    /// whose group interiors revert to the outer, unrestricted state. (The 6.5
    /// motivating consumer, the optional-group argument parser's brace protection,
    /// since detached in favor of the state-scoped temporary-group-rules
    /// ([`GroupRules::temporary`](crate::token::GroupRules::temporary)) lifecycle.)
    child_states: ChildStateSpec<'p, L>,
}

impl<'p, L: Lang> GroupParser<'p, L> {
    /// A parser for the group opened by the consumed `GroupOpen` token with span
    /// `open_span` and resolved rule `rule`, staging nodes with spans into the context's
    /// [`source`](super::ParseContext::source).
    pub fn new(open_span: Span, rule: Arc<GroupRule<L>>) -> GroupParser<'p, L> {
        GroupParser { open_span, rule, child_states: ChildStateSpec::inherit() }
    }

    /// Replace the interior's descent-state policy (default: inherit everywhere). See
    /// [`ChildStateSpec`].
    pub fn with_child_states(mut self, child_states: ChildStateSpec<'p, L>) -> Self {
        self.child_states = child_states;
        self
    }
}

impl<L: Lang> ConstructParser<L> for GroupParser<'_, L>
where
    L::InvocationSyntax: FromInvocation<L>,
{
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (BuildId, Option<Box<ParsingStateDelta<L>>>)> {
        // The interior state: base + expecting_group_close + the driver's descent
        // delta, session-memoized (one derivation per (base, rule); every descent
        // still reaches observe_transition).
        let interior_state = cx.group_interior_state(&self.rule)?;

        // Recurse under the interior state (structural swap/revert — the state-threading
        // convention). The stop condition names the exact pairing the group opened with,
        // and consumes the close at match time.
        let stop = StopSpec::at_token(
            TokenStopKind::GroupClose {
                group_type: self.rule.group_type,
                close: &self.rule.close,
            },
            true,
        );
        // The group-interior traceback frame ([§dd-dr:errors]): conditions detected inside the
        // group carry `group ‘{’` @ the open delimiter in their snapshot.
        let frame = Frame {
            title: FrameTitle::Quoted {
                label: "group",
                name: SourceSpan::new(&cx.source, self.open_span),
            },
            span: SourceSpan::new(&cx.source, self.open_span),
        };
        let child_states = self.child_states.clone();
        let (outcome, delta) = cx.with_frame(frame, |cx| {
            cx.parse_nodes(interior_state, stop, child_states)
        })?;
        // The content-loop parser comes from the driver's factory (outer-layer code
        // for a custom driver), so its contract is validated, not debug-asserted
        // ([§dd-dr:panic-policy]).
        if delta.is_some() {
            return Err(cx.implementation_error(
                "the driver's content-loop parser returned a pass-through state delta \
                 (a nodes parser has no after-effect to report)",
                Span::empty(cx.tokens.pos()),
            ));
        }

        let (close, end) = match outcome.stop {
            // The close was consumed at match time; its span becomes the recorded
            // delimiter (it cannot be re-peeked — hence the span on the cause).
            StopCause::TokenCondition { span } => (TextContent::Spanned(span), span.end()),
            StopCause::EndOfInput => {
                cx.recover(
                    UnclosedGroup::new(&*self.rule.close, UnclosedGroupFound::EndOfInput),
                    SourceSpan::new(&cx.source, self.open_span),
                )?;
                (TextContent::empty(), cx.tokens.pos())
            }
            // A close that matches neither field of the pairing: unwind — close this
            // group here, leave the token for an enclosing level (or the root) to claim.
            StopCause::UnexpectedGroupClose { span } => {
                cx.recover(
                    UnclosedGroup::new(&*self.rule.close, UnclosedGroupFound::StrayClose),
                    SourceSpan::new(&cx.source, span),
                )?;
                (TextContent::empty(), span.start())
            }
            StopCause::NodeCondition => {
                return Err(cx.implementation_error(
                    "the driver's content-loop parser reported a node-condition stop, \
                     but the group interior's stop spec sets no node condition",
                    Span::empty(cx.tokens.pos()),
                ));
            }
        };

        let data = GroupData {
            group_type: Some(self.rule.group_type),
            open: TextContent::Spanned(self.open_span),
            close,
        };
        let span = Span::new(self.open_span.start(), end);
        let id = cx.stage_node(
                NodeKind::group(data),
                SourceSpan::new(&cx.source, span),
                Arc::clone(&cx.state),
                outcome.nodes,
            )
            .map_err(|error| cx.staging_error(error, span))?;
        Ok((id, None))
    }
}

impl<L: Lang> core::fmt::Debug for GroupParser<'_, L> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GroupParser")
            .field("open_span", &self.open_span)
            .field("rule", &self.rule)
            .field("child_states", &self.child_states)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ParserSession;
    use crate::error::Recovery;
    use crate::scopes::ScopeStack;
    use crate::source::Source;
    use crate::state::{ParsingState, TrivialLang, StateData};
    use crate::token::{
        CommandRules, CommentRules, ForbiddenCharsRules, GroupRules, ParagraphRules,
        SpecialsRules, StdTokenReader, Token, TokenKind, TokenReader, TokenRules,
        WhitespaceRules,
    };
    use alloc::vec;
    use alloc::vec::Vec;

    #[derive(Debug, Clone, Copy)]
    struct TestLang;
    impl TrivialLang for TestLang {}

    fn rules() -> TokenRules<TestLang> {
        TokenRules {
            whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
            paragraphs: ParagraphRules { enabled: true },
            groups: GroupRules {
                enabled: true,
                rules: vec![Arc::new(GroupRule {
                    group_type: 0,
                    open: "{".into(),
                    close: "}".into(),
                })],
                temporary: Vec::new(),
                expecting_close: None,
            },
            commands: CommandRules {
                enabled: true,
                rules: Vec::new(),
            },
            comments: CommentRules {
                enabled: true,
                rules: Vec::new(),
            },
            specials: SpecialsRules { enabled: true },
            forbidden_chars: ForbiddenCharsRules { chars: "".into() },
        }
    }

    fn state() -> Arc<ParsingState<TestLang>> {
        Arc::new(ParsingState::new(StateData {
            rules: rules(),
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }))
    }

    /// Drive a lone `GroupParser` (no enclosing loop): peek the open token, consume it
    /// (the caller's job per the contract), run, freeze.
    fn parse_group(
        content: &str,
        recovery: Recovery,
    ) -> (crate::engine::ParseResult<TestLang>, usize) {
        let source: Arc<Source> = Arc::new(Source::new(content));
        let st = state();
        let mut reader = StdTokenReader::new(&source);
        let open: Token<'_, TestLang> = TokenReader::peek(&mut reader, &st).unwrap();
        let TokenKind::GroupOpen { rule, .. } = &open.kind else {
            panic!("test content must start with a group open")
        };
        let rule = Arc::clone(rule);
        reader.move_past(&open, true);
        let mut session = ParserSession::new();
        let driver = crate::engine::StdParseDriver::new(recovery, ());
        let mut cx = ParseContext::new(
            &mut reader,
            Arc::clone(&source),
            Arc::clone(&st),
            &mut session,
            &driver,
        );
        let mut parser = GroupParser::new(open.span, rule);
        let (id, delta) = parser.parse(&mut cx).unwrap();
        assert!(delta.is_none());
        let pos = cx.tokens.pos();
        (session.finish(id).unwrap(), pos)
    }

    #[test]
    fn direct_drive_round_trips_a_group() {
        let (result, pos) = parse_group("{ab} rest", Recovery::Strict);
        let root = result.tree.root();
        assert!(root.is_group());
        assert_eq!(root.span().range(), 0..4);
        assert_eq!(root.group_delimiters(), Some(("{", "}")));
        assert_eq!(root.group_type(), Some(0));
        assert_eq!(root.child_count(), 1);
        assert_eq!(root.child(0).unwrap().chars(), Some("ab"));
        // The close is consumed; the following space is *content* of the enclosing
        // level (a group has no post-space) and stays unread.
        assert_eq!(pos, 4);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn direct_drive_unclosed_group_recovers_with_empty_close() {
        let (result, pos) = parse_group("{ab", Recovery::Tolerant);
        let root = result.tree.root();
        assert_eq!(root.span().range(), 0..3);
        assert_eq!(root.group_delimiters(), Some(("{", "")));
        assert_eq!(root.child(0).unwrap().chars(), Some("ab"));
        assert_eq!(pos, 3);
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        // The wording stays covered by a Display assertion…
        assert_eq!(diagnostic.message(), "unclosed group: expected ‘}’ before end of input");
        // …while machine consumers read the structured condition ([§dd-dr:errors]).
        assert_eq!(diagnostic.identifier(), UnclosedGroup::IDENTIFIER);
        let condition = diagnostic.data().downcast_ref::<UnclosedGroup>().unwrap();
        assert_eq!(condition.expected_close, "}");
        assert_eq!(condition.found, UnclosedGroupFound::EndOfInput);
        // The diagnostic points at the open delimiter that was never closed.
        assert_eq!(diagnostic.span().range(), 0..1);
    }
}
