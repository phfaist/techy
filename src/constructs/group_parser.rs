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
//! # Matching and recovery (DESIGN_RATIONALE.md §3.5, §3.6, §3.8)
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
//!   environment-terminator mismatch (§3.6): every level either consumes the token or
//!   unwinds out of its own frame, and the stray close eventually reaches a level that
//!   claims it (or the root, which diagnoses and skips).
//!
//! Under [`Recovery::Strict`](crate::error::Recovery) both conditions abort instead.

use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;
use core::mem;

use crate::engine::{Frame, FrameTitle};
use crate::error::DiagnosticInfo;
use crate::node::{BuildId, GroupData, NodeKind};
use crate::source::{SourceSpan, Span, TextContent};
use crate::state::{Lang, ParsingStateDelta};
use crate::token::GroupRule;

use super::child_state::ChildStateSpec;
use super::nodes_parser::{NodesParser, StopCause, StopSpec, TokenStopKind};
use super::{ConstructParser, ConstructParserResult, ParseContext};

/// Condition: a delimited group was never closed with its expected delimiter — detected
/// by [`GroupParser`], which defines the condition next to its detection site
/// (DESIGN_RATIONALE.md §3.8).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct UnclosedGroup {
    /// The close delimiter the group expected (as written, e.g. `}`).
    pub expected_close: String,
    /// What blocked the close instead.
    pub found: UnclosedGroupFound,
}

/// What an [`UnclosedGroup`] ran into instead of its close delimiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnclosedGroupFound {
    /// The input ended inside the group.
    EndOfInput,
    /// A close delimiter belonging to a different pairing appeared (the group unwinds,
    /// leaving the stray token for an enclosing level to claim).
    StrayClose,
}

impl UnclosedGroup {
    /// The condition for a group expecting `expected_close`.
    pub fn new(expected_close: impl Into<String>, found: UnclosedGroupFound) -> UnclosedGroup {
        UnclosedGroup { expected_close: expected_close.into(), found }
    }
}

impl fmt::Display for UnclosedGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.found {
            UnclosedGroupFound::EndOfInput => write!(
                f,
                "unclosed group: expected ‘{}’ before end of input",
                self.expected_close
            ),
            UnclosedGroupFound::StrayClose => {
                write!(f, "unclosed group: expected ‘{}’", self.expected_close)
            }
        }
    }
}

impl DiagnosticInfo for UnclosedGroup {
    const IDENTIFIER: &'static str = "core.group_parser.unclosed-group";
}

/// The group construct parser: a tier-2 temporary, constructed per group descent from
/// the opening token's facts (see the module docs for the contract).
pub struct GroupParser<'p, L: Lang> {
    /// The opening delimiter's span (the consumed `GroupOpen` token's span).
    open_span: Span,
    /// The opening token's resolved rule: the close spelling and group class of the
    /// pairing to match.
    rule: Arc<GroupRule<L>>,
    /// Descent-state policy handed to the interior [`NodesParser`]; defaults to
    /// inherit-everywhere (§3.6 decided semantics 3: policies are one level deep, so a
    /// plain arm-driven descent never propagates one). A parser that scopes the group's
    /// interior state sets it per use — the optional-group argument parser's
    /// brace-protection policy is the motivating consumer
    /// ([`OptionalGroupArgumentParser`](super::OptionalGroupArgumentParser)).
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

impl<L: Lang> ConstructParser<L> for GroupParser<'_, L> {
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (BuildId, Option<ParsingStateDelta<L>>)> {
        // The interior state: base + expecting_group_close, session-memoized (one
        // derivation per (base, rule); every descent still reaches observe_transition).
        let interior_state = cx.session.group_interior_state(&cx.state, &self.rule);

        // Recurse under the interior state (structural swap/revert — §2 state-threading
        // convention). The stop condition names the exact pairing the group opened with,
        // and consumes the close at match time.
        let stop = StopSpec::at_token(
            TokenStopKind::GroupClose {
                group_type: self.rule.group_type,
                close: &self.rule.close,
            },
            true,
        );
        let mut interior = NodesParser::new(stop).with_child_states(self.child_states.clone());
        // The group-interior traceback frame (§3.8): conditions detected inside the
        // group carry `group ‘{’` @ the open delimiter in their snapshot.
        let frame = Frame {
            title: FrameTitle::Quoted {
                label: "group",
                name: SourceSpan::new(&cx.source, self.open_span.range()),
            },
            span: SourceSpan::new(&cx.source, self.open_span.range()),
        };
        let outer_state = mem::replace(&mut cx.state, interior_state);
        let result = cx.with_frame(frame, |cx| interior.parse(cx));
        cx.state = outer_state;
        let (outcome, delta) = result?;
        debug_assert!(delta.is_none(), "NodesParser returns no pass-through delta");

        let (close, end) = match outcome.stop {
            // The close was consumed at match time; its span becomes the recorded
            // delimiter (it cannot be re-peeked — hence the span on the cause).
            StopCause::TokenCondition { span } => (TextContent::Spanned(span), span.end),
            StopCause::EndOfInput => {
                cx.recover(
                    UnclosedGroup::new(&*self.rule.close, UnclosedGroupFound::EndOfInput),
                    SourceSpan::new(&cx.source, self.open_span.range()),
                )?;
                (TextContent::empty(), cx.tokens.pos())
            }
            // A close that matches neither field of the pairing: unwind — close this
            // group here, leave the token for an enclosing level (or the root) to claim.
            StopCause::UnexpectedGroupClose { span } => {
                cx.recover(
                    UnclosedGroup::new(&*self.rule.close, UnclosedGroupFound::StrayClose),
                    SourceSpan::new(&cx.source, span.range()),
                )?;
                (TextContent::empty(), span.start)
            }
            StopCause::NodeCondition => {
                unreachable!("the group parser sets no node stop condition")
            }
        };

        let data = GroupData {
            group_type: Some(self.rule.group_type),
            open: TextContent::Spanned(self.open_span),
            close,
            ext: Default::default(),
        };
        let span = Span::new(self.open_span.start, end);
        let id = cx.session.builder.add(
            NodeKind::group(data),
            SourceSpan::new(&cx.source, span.range()),
            Arc::clone(&cx.state),
            outcome.nodes,
        );
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
    use crate::library::LibraryStack;
    use crate::source::Source;
    use crate::state::{ParsingState, SimpleLang, StateData};
    use crate::token::{
        StdTokenReader, Token, TokenKind, TokenReader, TokenRules, WhitespaceRules,
    };
    use alloc::vec;
    use alloc::vec::Vec;

    #[derive(Debug, Clone, Copy)]
    struct TestLang;
    impl SimpleLang for TestLang {}

    fn rules() -> TokenRules<TestLang> {
        TokenRules {
            enable_whitespace: true,
            whitespace: WhitespaceRules { chars: " \t\n".into() },
            enable_multi_newline_paragraphs: true,
            enable_groups: true,
            groups: vec![Arc::new(GroupRule {
                group_type: 0,
                open: "{".into(),
                close: "}".into(),
            })],
            enable_commands: true,
            commands: Vec::new(),
            enable_comments: true,
            comments: Vec::new(),
            enable_specials: true,
            forbidden_chars: "".into(),
            expecting_group_close: None,
        }
    }

    fn state() -> Arc<ParsingState<TestLang>> {
        Arc::new(ParsingState::new(StateData {
            rules: rules(),
            libraries: LibraryStack::new(),
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
        let mut reader = StdTokenReader::new(content);
        let open: Token<'_, TestLang> = TokenReader::peek(&mut reader, &st).unwrap();
        let TokenKind::GroupOpen { rule, .. } = &open.kind else {
            panic!("test content must start with a group open")
        };
        let rule = Arc::clone(rule);
        reader.move_past(&open, true);
        let mut session = ParserSession::new(recovery);
        let mut cx = ParseContext {
            tokens: &mut reader,
            source: Arc::clone(&source),
            state: Arc::clone(&st),
            session: &mut session,
        };
        let mut parser = GroupParser::new(open.span, rule);
        let (id, delta) = parser.parse(&mut cx).unwrap();
        assert!(delta.is_none());
        let pos = cx.tokens.pos();
        (session.finish(id), pos)
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
        // …while machine consumers read the structured condition (§3.8).
        assert_eq!(diagnostic.identifier(), UnclosedGroup::IDENTIFIER);
        let condition = diagnostic.data().downcast_ref::<UnclosedGroup>().unwrap();
        assert_eq!(condition.expected_close, "}");
        assert_eq!(condition.found, UnclosedGroupFound::EndOfInput);
        // The diagnostic points at the open delimiter that was never closed.
        assert_eq!(diagnostic.span().range(), 0..1);
    }
}
