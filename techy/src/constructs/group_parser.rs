//! [`GroupParser`]: parses one delimited group — interior content up to and including
//! the close delimiter — into a `Group` node (pylatexenc's `LatexDelimitedGroupParser`).
//!
//! # Contract
//!
//! Constructed with the opening delimiter's token and its resolved
//! [`GroupRule`] — the token to ask the reader about the delimiter, the rule as the
//! [`GroupOpen`](crate::token::TokenKind::GroupOpen) view reported it. The **caller
//! consumes that token** before running the
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
//! restored after, so a state change made inside the group ends with it.
//!
//! # Leaking an after-effect (the `\gdef` shape)
//!
//! By default the returned after-effect delta is `None`: the interior run's merged
//! record ([`NodesOutcome::after_effects`](super::NodesOutcome::after_effects)) is
//! dropped with the descent, which is what makes a definition group-scoped. A language
//! whose constructs may *escape* their enclosing group — TeX's `\gdef` — installs the
//! [`GroupAfterEffectsFn`] hook
//! ([`new_with_after_effects`](GroupParser::new_with_after_effects), from an overridden
//! [`make_group_parser`](crate::engine::ParseDriver::make_group_parser)): it maps the
//! interior record to the group's own after-effect for its caller. What the hook can and
//! cannot discriminate is documented on the type.
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
use crate::error::{DiagnosticInfo, ParseError, ToDiagnosticValue};
use crate::node::{BuildId, GroupData, NodeKind};
use crate::source::TextContent;
use crate::state::{Lang, ParsingState, ParsingStateDelta};
use crate::token::{GroupRule, Token, TokenEdge};

use super::child_state::ChildStateSpec;
use super::nodes_parser::{StopCause, StopSpec, TokenStopKind};
use super::{node_text_content, ConstructParser, ConstructParserResult, FromInvocation, ParseContext};

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

/// The hook by which a group **leaks an after-effect** to its caller: it maps the
/// interior content run's merged after-effect record to the delta the group returns as
/// its own after-effect (the `\gdef` shape). Installed per descent with
/// [`GroupParser::new_with_after_effects`], normally from an overridden
/// [`make_group_parser`](crate::engine::ParseDriver::make_group_parser) so that every
/// group descent of the language carries it.
///
/// The arguments, in order:
///
/// 1. The **matched rule** — the pairing the group opened with, so a hook can key on the
///    group *class*: a language may let `\gdef` escape a `{…}` group but nothing escape
///    a `$…$` one.
/// 2. The **interior's initial state** — what the interior parsed under before any
///    sibling after-effect evolved it (base + `expecting_group_close` + the driver's
///    [`group_interior_delta`](crate::engine::ParseDriver::group_interior_delta)).
/// 3. The **interior's exit state** ([`NodesOutcome::state`](super::NodesOutcome::state))
///    — the state the interior run actually reached, and the only place the definitions
///    the record made are inspectable (`state.scopes().retrieve_spec(…)`). It is
///    discarded with the descent; the hook is the last reader.
/// 4. The **interior's merged record**
///    ([`NodesOutcome::after_effects`](super::NodesOutcome::after_effects)), passed **by
///    value** so the hook may filter it in place and hand the same box back rather than
///    cloning.
///
/// The return is the group's after-effect for its caller — `None` for the ordinary
/// "nothing escapes" answer, which is also what a group with no hook returns.
///
/// # What a hook can discriminate
///
/// The record is **one merged delta** — rules overrides last-writer-wins, scope ops and
/// events concatenated in application order — and carries no provenance: it cannot say which construct contributed what. A
/// `\gdef`-vs-`\def` split is therefore expressed **structurally**, by the language
/// tagging its own ops — `\gdef` emitting a [`ScopeOp::Define`](crate::scopes::ScopeOp)
/// against a globally-named scope, `\def` a local one — after which the hook keeps the
/// globally-targeted ops and drops the rest (the ops read through
/// `<L::Features as LangFeatures>::Scopes::store_get(&delta.scope_ops)`). The rules,
/// mode and ext overrides carry no such tag and merge last-writer-wins, so for those the
/// only honest answers are all or nothing.
///
/// Escapes **compose outward** one level per hook: the enclosing content loop applies the
/// returned delta to its own state *and* merges it into its own record, so the next group
/// out sees it in argument 4 and may let it escape again.
///
/// # Errors
///
/// `Err` **aborts the parse** under any recovery policy, propagated exactly like a
/// construct parser's own `Err`, with the live traceback attached at the descent site —
/// the callback has no session access. Same condition choice as
/// [`GroupChildState::Compute`](super::GroupChildState::Compute):
/// [`HookFailed`](crate::error::HookFailed) for an operational failure in the callback's
/// own code, [`ImplementationError`](super::ImplementationError) for a violated library
/// contract. An infallible hook wraps its answer in `Ok(…)` and that is the only change.
///
/// Deterministic and side-effect free, like the [`ChildStateSpec`] callbacks: a hook
/// whose answer depends on call order would be fragile.
#[allow(clippy::type_complexity)]
pub type GroupAfterEffectsFn<'p, L> = &'p dyn Fn(
    &Arc<GroupRule<L>>,
    &Arc<ParsingState<L>>,
    &Arc<ParsingState<L>>,
    Option<Box<ParsingStateDelta<L>>>,
) -> Result<
    Option<Box<ParsingStateDelta<L>>>,
    ParseError<<L as Lang>::SourceOrigin>,
>;

/// The group construct parser: a tier-2 temporary, constructed per group descent
/// from the opening token's facts (its span and resolved
/// [`GroupRule`](crate::token::GroupRule)). The caller has already consumed the
/// opening token; `cx.state` is the interior's **base** state, from which the parser
/// derives the actual interior state (base plus the expected close delimiter from
/// the opening rule), scoped structurally over the descent. Recovery for a group
/// that never closes is documented on [`UnclosedGroup`].
pub struct GroupParser<'p, L: Lang> {
    /// The consumed `GroupOpen` token: the group's open delimiter, as the reader
    /// records it (its span and its stream position come from the reader).
    open: Token<L>,
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
    /// The after-effect leak hook, `None` for the scoped-descent default (nothing
    /// escapes the group). See [`GroupAfterEffectsFn`].
    compute_after_effects: Option<GroupAfterEffectsFn<'p, L>>,
}

impl<'p, L: Lang> GroupParser<'p, L> {
    /// A parser for the group opened by the consumed `GroupOpen` token `open`, with
    /// resolved rule `rule`.
    pub fn new(open: Token<L>, rule: Arc<GroupRule<L>>) -> GroupParser<'p, L> {
        GroupParser {
            open,
            rule,
            child_states: ChildStateSpec::inherit(),
            compute_after_effects: None,
        }
    }

    /// [`new`](GroupParser::new) with the **after-effect leak hook** installed: instead
    /// of dropping the interior run's merged after-effect record with the descent, the
    /// parser hands it to `compute_after_effects`, whose answer becomes the group's own
    /// after-effect for its caller (the `\gdef` shape). The hook may still answer `None`,
    /// which is the default behavior.
    ///
    /// The hook is per descent, so the plug-in point is an overridden
    /// [`make_group_parser`](crate::engine::ParseDriver::make_group_parser) — that is what
    /// gives a language the hook at *every* group site, which is in turn what makes an
    /// escape compose outward through nested groups. The full contract, including what
    /// the merged record can and cannot discriminate, is on [`GroupAfterEffectsFn`].
    pub fn new_with_after_effects(
        open: Token<L>,
        rule: Arc<GroupRule<L>>,
        compute_after_effects: GroupAfterEffectsFn<'p, L>,
    ) -> GroupParser<'p, L> {
        GroupParser {
            open,
            rule,
            child_states: ChildStateSpec::inherit(),
            compute_after_effects: Some(compute_after_effects),
        }
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
        let open_span = cx.tokens.source_span_of(&self.open);
        let frame = Frame {
            title: FrameTitle::Quoted { label: "group", name: open_span.clone() },
            span: open_span.clone(),
        };
        let child_states = self.child_states.clone();
        // The interior state is cloned into the descent: the hook (when installed) reads
        // the state the interior *started* under, which the run itself no longer holds
        // once a sibling after-effect has evolved it.
        let (outcome, delta) = cx.with_frame(frame, |cx| {
            cx.parse_nodes(Arc::clone(&interior_state), stop, child_states)
        })?;
        // The content-loop parser comes from the driver's factory (outer-layer code
        // for a custom driver), so its contract is validated, not debug-asserted
        // ([§dd-dr:panic-policy]).
        if delta.is_some() {
            return Err(cx.implementation_error(
                "the driver's content-loop parser returned a pass-through state delta \
                 (a nodes parser has no after-effect to report)",
                cx.here(),
            ));
        }

        // The close delimiter's own span, and the stream position the group ends at.
        let (close_span, end) = match outcome.stop {
            // The close was consumed at match time; its span becomes the recorded
            // delimiter (it cannot be re-peeked — hence the span on the cause).
            StopCause::TokenCondition { span, after } => (Some(span), after),
            StopCause::EndOfInput => {
                cx.recover(
                    UnclosedGroup::new(&*self.rule.close, UnclosedGroupFound::EndOfInput),
                    open_span.clone(),
                )?;
                (None, cx.tokens.position_here())
            }
            // A close that matches neither field of the pairing: unwind — close this
            // group here, leave the token for an enclosing level (or the root) to claim.
            StopCause::UnexpectedGroupClose { span, .. } => {
                cx.recover(
                    UnclosedGroup::new(&*self.rule.close, UnclosedGroupFound::StrayClose),
                    span,
                )?;
                (None, cx.tokens.position_here())
            }
            StopCause::NodeCondition => {
                return Err(cx.implementation_error(
                    "the driver's content-loop parser reported a node-condition stop, \
                     but the group interior's stop spec sets no node condition",
                    cx.here(),
                ));
            }
        };

        let span = cx
            .source_span_within(&cx.tokens.position_at(&self.open, TokenEdge::Start), &end)?;
        // The recorded delimiters are node data: a node-relative span of the node's
        // own source, or the delimiter's text when it comes from another source.
        let open = node_text_content(&open_span, &span);
        let close = match &close_span {
            Some(close_span) => node_text_content(close_span, &span),
            None => TextContent::empty(),
        };
        let data = GroupData { group_type: Some(self.rule.group_type), open, close };
        let id = cx
            .stage_node(
                NodeKind::group(data),
                span.clone(),
                Arc::clone(&cx.state),
                outcome.nodes,
            )
            .map_err(|error| cx.staging_error(error, span))?;

        // The group's after-effect for its caller. With no hook installed the interior
        // run's merged record dies with the descent (`None` — the scoped-descent default,
        // which is what makes a definition group-scoped); with one, the hook decides what
        // escapes ([`GroupAfterEffectsFn`]). A hook `Err` aborts under any recovery
        // policy, with the live traceback attached here — the callback has no session
        // access.
        let after_effects = match self.compute_after_effects {
            None => None,
            Some(compute) => compute(
                &self.rule,
                &interior_state,
                &outcome.state,
                outcome.after_effects,
            )
            .map_err(|error| cx.attach_hook_frames(error))?,
        };
        Ok((id, after_effects))
    }
}

impl<L: Lang> core::fmt::Debug for GroupParser<'_, L> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GroupParser")
            .field("open", &self.open)
            .field("rule", &self.rule)
            .field("child_states", &self.child_states)
            .field(
                "compute_after_effects",
                match &self.compute_after_effects {
                    Some(_) => &"Some(..)",
                    None => &"None",
                },
            )
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
        SpecialsRules, StdToken, StdTokenReader, TokenKind, TokenReader, TokenRules,
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
        let open: StdToken<TestLang> = TokenReader::peek(&mut reader, &st).unwrap();
        let reader_ref: &dyn TokenReader<'_, TestLang> = &reader;
        let TokenKind::GroupOpen { rule, .. } = reader_ref.token_kind(&open) else {
            panic!("test content must start with a group open")
        };
        let rule = Arc::clone(rule);
        TokenReader::<'_, TestLang>::move_to(&mut reader, &open, TokenEdge::EndPastPostSpace);
        let mut session = ParserSession::new();
        let driver = crate::engine::StdParseDriver::new(recovery, ());
        let mut cx = ParseContext::new(
            &mut reader,
            Arc::clone(&st),
            &mut session,
            &driver);
        let mut parser = GroupParser::new(open.clone(), rule);
        let (id, delta) = parser.parse(&mut cx).unwrap();
        assert!(delta.is_none());
        // The reader's position, as a byte offset, through its own answer.
        let pos = cx.here().start();
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
    fn the_after_effects_hook_sees_the_interior_and_its_answer_reaches_the_caller() {
        // Direct drive with the leak hook installed: the group's returned delta is the
        // hook's answer verbatim, and the hook sees the interior's two states. `{ab}`
        // holds no construct with an after-effect, so the record arrives as `None` and
        // the interior never left the state it started in.
        let source: Arc<Source> = Arc::new(Source::new("{ab}"));
        let st = state();
        let mut reader = StdTokenReader::new(&source);
        let open: StdToken<TestLang> = TokenReader::peek(&mut reader, &st).unwrap();
        let reader_ref: &dyn TokenReader<'_, TestLang> = &reader;
        let TokenKind::GroupOpen { rule, .. } = reader_ref.token_kind(&open) else {
            panic!("test content must start with a group open")
        };
        let rule = Arc::clone(rule);
        TokenReader::<'_, TestLang>::move_to(&mut reader, &open, TokenEdge::EndPastPostSpace);
        let mut session = ParserSession::new();
        let driver = crate::engine::StdParseDriver::new(Recovery::Strict, ());
        let mut cx = ParseContext::new(&mut reader, Arc::clone(&st), &mut session, &driver);

        let seen = core::cell::Cell::new(0usize);
        let hook = |seen_rule: &Arc<GroupRule<TestLang>>,
                    initial: &Arc<ParsingState<TestLang>>,
                    exit: &Arc<ParsingState<TestLang>>,
                    record: Option<Box<ParsingStateDelta<TestLang>>>| {
            seen.set(seen.get() + 1);
            assert_eq!(seen_rule.group_type, 0);
            // The interior parsed under the derived expecting-close state, not the
            // group's own input state…
            assert!(!Arc::ptr_eq(initial, &st));
            // …and nothing evolved it, so the run exited where it started.
            assert!(Arc::ptr_eq(initial, exit));
            assert!(record.is_none());
            // An answer the caller could not have produced itself, to pin the channel.
            Ok(Some(Box::new(ParsingStateDelta::new().mode(()))))
        };
        let mut parser = GroupParser::new_with_after_effects(open.clone(), rule, &hook);
        let (id, delta) = parser.parse(&mut cx).unwrap();
        assert_eq!(seen.get(), 1);
        assert!(delta.is_some());

        let result = session.finish(id).unwrap();
        assert!(result.tree.root().is_group());
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
