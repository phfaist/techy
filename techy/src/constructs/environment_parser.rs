//! [`EnvironmentBodyParser`]: parses an environment-shaped callable's body — content up
//! to and including the rigid `\end{name}` terminator — staging it as one body `List`
//! node (Phase 6.6; pylatexenc's environment body handling, expressed as the decided
//! parameterized core parser).
//!
//! # Design (DESIGN_RATIONALE.md §3.5, §3.6)
//!
//! **Slot terminators are parser business** — and slots have no spec-side declaration
//! at all (decided July 2026, slots session): the terminator data (the stop command's
//! name, the name group's class, whether the name must back-reference the invocation)
//! parameterizes this parser, and the composition driving it mints the
//! [`ParsedSlot`](crate::node::ParsedSlot) records directly. A body state delta
//! (pylatexenc's `make_body_parsing_state_delta`) lives as an ordinary field on the
//! preset spec type that drives the parse (Phase 7's `EnvironmentSpec`) — the core
//! never interprets it. Environments stay zero-custom-code for spec authors.
//!
//! **The scaffolding is rigid and reconstructed, not recorded** (decision 6): the
//! terminator is the stop command followed **immediately** by its name group — no
//! comments, no paragraph breaks, no whitespace inside the name; the command token's own
//! syntactic post-space (`\end {name}`) is the one tolerated, unrecorded gap. The
//! consumed terminator bytes appear in no node: they are the reconstructible complement
//! between the body `List`'s end and the callable's span end.
//!
//! **The caller scopes the body's state**: `cx.state` is the slot's state — the driving
//! invocation parser stacks the slot's `parsing_state_delta` on the invocation's base
//! (session-mediated) and reverts structurally, exactly as for arguments. Both the body
//! content *and the terminator* are read under it; a slot state that cannot tokenize the
//! stop command runs the body to end of input (the §3.6 pitfall — environments do not
//! self-heal the way group closes do).
//!
//! # Recovery (decision 8; DESIGN_RATIONALE.md §3.8)
//!
//! - *Name mismatch* (`\begin{A}…\begin{B}…\end{A}`): diagnostic + close the body
//!   **without consuming** the terminator — the unwinding lets the enclosing
//!   environment's parser find and consume its own `\end{A}`; an orphan terminator
//!   eventually reaches the root loop as an ordinary command and takes the
//!   unresolvable-command recovery.
//! - *Malformed terminator* (no rigid name group after the stop command): diagnostic +
//!   consume the command **alone** + close — leaving it unconsumed would cascade the
//!   same malformed token through every enclosing level; what follows it is left as
//!   enclosing content.
//! - *End of input*: missing-terminator diagnostic (anchored at the invocation trigger,
//!   the group parser's unclosed-at-open precedent) + close.
//! - *Unexpected group close in the body*: diagnostic + close without consuming — every
//!   level consumes or unwinds, and the stray close is left for an enclosing level (or
//!   the root) to claim.
//!
//! Under [`Recovery::Strict`](crate::error::Recovery) each condition aborts instead.
//! "Was this environment properly terminated?" lives in the diagnostics, not on the
//! node — a preset wanting it on the node flags it in ext via `Lang::finalize_node`.

use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;

use crate::engine::{Frame, FrameTitle};
use crate::error::{DiagnosticInfo, ToDiagnosticValue};
use crate::node::{BuildId, NodeKind};
use crate::source::{SourceSpan, Span};
use crate::state::{Lang, ParsingStateDelta};
use crate::token::{GroupRule, TokenKind};

use super::nodes_parser::{NodesParser, StopCause, StopSpec, TokenStopKind};
use super::{ConstructParser, ConstructParserResult, ParseContext};

/// Condition: an environment's body ran into the terminator of a *different*
/// environment (`\begin{A}…\end{B}`) — the body closes without consuming it, unwinding
/// so an enclosing level can claim its own terminator (decision 8,
/// DESIGN_RATIONALE.md §3.8).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.environment_parser.terminator-mismatch",
    message = "missing terminator of environment ‘{expected}’: found the terminator of \
               ‘{found}’ instead"
)]
pub struct EnvironmentTerminatorMismatch {
    /// The environment being parsed, whose terminator was expected.
    pub expected: String,
    /// The environment named by the terminator that was found instead.
    pub found: String,
}

/// Condition: the terminator command was not followed immediately by its rigid name
/// group (`\end[y]`, `\end{ A }`) — the command alone is consumed and the body closes.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.environment_parser.malformed-terminator",
    message = "malformed terminator of environment ‘{environment}’: expected its name \
               group immediately after the command"
)]
pub struct MalformedEnvironmentTerminator {
    /// The environment being parsed.
    pub environment: String,
}

/// Condition: an environment's body ended without its terminator ever appearing — at
/// end of input, or unwound by a stray group close nobody at the body's level asked for.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.environment_parser.missing-terminator")]
pub struct MissingEnvironmentTerminator {
    /// The environment being parsed.
    pub environment: String,
    /// What ended the body instead.
    pub found: MissingTerminatorFound,
}

/// What ended a body missing its terminator ([`MissingEnvironmentTerminator`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToDiagnosticValue)]
#[non_exhaustive]
pub enum MissingTerminatorFound {
    /// The input ended inside the body.
    EndOfInput,
    /// A group close belonging to an enclosing level appeared (the body unwinds,
    /// leaving the token for that level to claim).
    StrayGroupClose,
}

// Hand-written wording: the message varies by what ended the body (a match, which the
// message format string cannot express).
impl fmt::Display for MissingEnvironmentTerminator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.found {
            MissingTerminatorFound::EndOfInput => write!(
                f,
                "missing terminator of environment ‘{}’ before end of input",
                self.environment
            ),
            MissingTerminatorFound::StrayGroupClose => {
                write!(f, "missing terminator of environment ‘{}’", self.environment)
            }
        }
    }
}

/// A successfully read rigid name group: the name's byte span (the exact content between
/// the delimiters, possibly empty) and the position just past the close delimiter.
#[derive(Debug, Clone, Copy)]
pub struct NameGroup {
    /// The name's span between the delimiters.
    pub name_span: Span,
    /// The position just past the group's close delimiter.
    pub end: usize,
}

/// Try to read a **rigid** environment name group at the reader's position (decision 6,
/// DESIGN_RATIONALE.md §3.5): a group open of class `name_group_type` as the immediately
/// next token (no pre-space — inline whitespace after the introducing command is that
/// token's own post-space, already consumed with it; comments and paragraph breaks are
/// not tolerated), a chars-only interior with no whitespace, and the matching close
/// delimiter.
///
/// On success the whole group is consumed and its facts returned. On **any** deviation
/// nothing is consumed — the reader is rewound to where it stood — and `None` is
/// returned; the caller applies its own recovery (the syntax is deliberately rigid, so
/// "not a name group" needs no finer distinction). A tokenizer error while reading
/// aborts under strict recovery; under tolerant it reports `None` without diagnosing or
/// consuming — the enclosing content loop re-reads the error and applies its own token
/// recovery (the argument-probe rule, [`ParseContext::probe_token`]).
///
/// Public as a takeover-composition building block (decided July 2026, slots session):
/// a `\begin`-shaped `make_invocation_parser` override reads its scaffolding with this
/// before resolving the environment and driving [`EnvironmentBodyParser`].
pub fn read_rigid_name_group<L: Lang>(
    cx: &mut ParseContext<'_, '_, L>,
    name_group_type: L::GroupTypeId,
) -> ConstructParserResult<L, Option<NameGroup>> {
    let entry = cx.tokens.pos();
    let Some(open) = cx.probe_token(&Arc::clone(&cx.state))? else {
        return Ok(None);
    };
    let rule = match &open.kind {
        TokenKind::GroupOpen { rule, .. }
            if rule.group_type == name_group_type && open.pre_space.is_empty() =>
        {
            Arc::clone(rule)
        }
        _ => return Ok(None),
    };
    cx.tokens.move_past(&open, true);

    // The interior tokens are read under base + expecting_group_close (the memoized
    // derivation the group parser uses), so the close delimiter is guaranteed
    // recognizable regardless of the base's delimiter table.
    let interior_state = cx.session.group_interior_state(&cx.state, &rule);
    let result = cx.with_scoped_state(interior_state, |cx| read_name_chars(cx, &rule));
    match result? {
        Some(name_group) => Ok(Some(name_group)),
        None => {
            cx.tokens.move_to_pos(entry);
            Ok(None)
        }
    }
}

/// The interior loop of [`read_rigid_name_group`]: consecutive `Char` tokens — no
/// whitespace anywhere, rigid — up to the close delimiter of `rule`.
fn read_name_chars<L: Lang>(
    cx: &mut ParseContext<'_, '_, L>,
    rule: &GroupRule<L>,
) -> ConstructParserResult<L, Option<NameGroup>> {
    let name_start = cx.tokens.pos();
    let mut name_end = name_start;
    let state = Arc::clone(&cx.state);
    loop {
        let Some(token) = cx.probe_token(&state)? else {
            return Ok(None);
        };
        if !token.pre_space.is_empty() {
            return Ok(None);
        }
        match &token.kind {
            TokenKind::Char(_) => {
                name_end = token.span.end();
                cx.tokens.move_past(&token, true);
            }
            TokenKind::GroupClose { delim } if **delim == *rule.close => {
                cx.tokens.move_past(&token, true);
                return Ok(Some(NameGroup {
                    name_span: Span::new(name_start, name_end),
                    end: token.span.end(),
                }));
            }
            _ => return Ok(None),
        }
    }
}

/// What [`EnvironmentBodyParser`] produces.
#[derive(Debug, Clone, Copy)]
pub struct EnvironmentBody {
    /// The staged body `List` node: span = the body's content interior, children = the
    /// body's nodes (an empty body is an empty `List` — a region that exists).
    pub body: BuildId,
    /// The position just past the environment's extent: past the consumed `\end{name}`
    /// terminator — or, when the body closed without consuming one (name mismatch,
    /// unexpected group close, end of input), the body's end. The driving invocation
    /// parser ends the callable's span here.
    pub end: usize,
}

/// The environment-body construct parser: a tier-2 temporary constructed by the
/// invocation parser driving the environment shape (see the module docs for the design
/// and recovery contract).
///
/// Runs at the position right after the environment's scaffolding and arguments; parses
/// sibling content up to the terminator command, handles the terminator per decision 8,
/// and stages the body `List`. Returns no after-effect delta.
pub struct EnvironmentBodyParser<'p, L: Lang> {
    /// The invocation trigger's span (`\begin{name}`'s command token), anchoring the
    /// missing-terminator diagnostic (the group parser's unclosed-at-open precedent).
    trigger_span: Span,
    /// The invocation name the terminator must back-reference (`align` for
    /// `\begin{align} … \end{align}`), and the name diagnostics call the environment.
    invocation_name: &'p str,
    /// The terminator command's name (`end`), the body loop's stop condition.
    stop_command_name: &'p str,
    /// The group class of the terminator's name group.
    name_group_type: L::GroupTypeId,
    /// Whether the terminator's name must match `invocation_name` (default: `true`).
    match_invocation_name: bool,
    /// The span of the invocation name as written (`align` in `\begin{align}`), when
    /// the caller has one: it titles the body's traceback frame (`environment ‘align’`).
    /// Without it the frame falls back to the generic "environment body".
    /// (`invocation_name` itself is a borrowed `&str` and cannot ride in the
    /// allocation-free live frame — a span can, §3.8.)
    invocation_name_span: Option<Span>,
}

impl<'p, L: Lang> EnvironmentBodyParser<'p, L> {
    /// A body parser for the environment invoked as `invocation_name` (trigger token
    /// span `trigger_span`), terminated by the command `stop_command_name` followed by
    /// its rigid name group of class `name_group_type`.
    pub fn new(
        trigger_span: Span,
        invocation_name: &'p str,
        stop_command_name: &'p str,
        name_group_type: L::GroupTypeId,
    ) -> EnvironmentBodyParser<'p, L> {
        EnvironmentBodyParser {
            trigger_span,
            invocation_name,
            stop_command_name,
            name_group_type,
            match_invocation_name: true,
            invocation_name_span: None,
        }
    }

    /// Set whether the terminator's name must match the invocation name (default:
    /// `true`). Disabled, any rigid name group closes the body — for constructs whose
    /// terminator does not back-reference the opening.
    pub fn with_match_invocation_name(mut self, match_invocation_name: bool) -> Self {
        self.match_invocation_name = match_invocation_name;
        self
    }

    /// Provide the span of the invocation name as written in the source, so the body's
    /// traceback frame can quote it (`environment ‘align’`). Drivers that read the name
    /// from a name group pass that group's interior span.
    pub fn with_invocation_name_span(mut self, name_span: Span) -> Self {
        self.invocation_name_span = Some(name_span);
        self
    }

    /// The terminator flow, entered with the matched stop command left unconsumed at
    /// its span start: read the rigid name group that must follow immediately, verify
    /// the back-reference, and consume — or unwind — per decision 8 (module docs).
    /// Returns the environment's end position.
    fn finish_terminator(
        &self,
        cx: &mut ParseContext<'_, '_, L>,
        body_end: usize,
    ) -> ConstructParserResult<L, usize> {
        // Re-read the stop token (its pre-space is already flushed as body content, so
        // it sits at its own span start). It was cleanly read under this same state
        // moments ago, so this cannot fail; the defensive arm only guards a misbehaving
        // custom reader.
        let Some(end_token) = cx.probe_token(&Arc::clone(&cx.state))? else {
            debug_assert!(false, "the matched stop token disappeared on re-peek");
            return Ok(body_end);
        };
        debug_assert!(
            matches!(&end_token.kind, TokenKind::Command { name, .. }
                if *name == self.stop_command_name),
            "the matched stop token changed kind on re-peek"
        );
        // Consume the command whole (syntactic post-space included — `\end {name}`'s
        // tolerated inline whitespace); the name group must be the very next token.
        cx.tokens.move_past(&end_token, true);
        let after_command = cx.tokens.pos();

        match read_rigid_name_group(cx, self.name_group_type)? {
            Some(name_group) => {
                let source = Arc::clone(&cx.source);
                let name = &source.content()[name_group.name_span.range()];
                if !self.match_invocation_name || name == self.invocation_name {
                    Ok(name_group.end)
                } else {
                    // Mismatch: close without consuming — rewind to the terminator
                    // command's start and leave the whole terminator for an enclosing
                    // level.
                    cx.recover(
                        EnvironmentTerminatorMismatch::new(self.invocation_name, name),
                        SourceSpan::new(&cx.source, end_token.span.start()..name_group.end),
                    )?;
                    cx.tokens.move_to(&end_token, false);
                    Ok(body_end)
                }
            }
            None => {
                // Malformed terminator: consume the command alone (the failed name
                // group read consumed nothing — the reader stands just past the
                // command) and close.
                debug_assert_eq!(cx.tokens.pos(), after_command);
                cx.recover(
                    MalformedEnvironmentTerminator::new(self.invocation_name),
                    SourceSpan::new(&cx.source, end_token.span),
                )?;
                Ok(after_command)
            }
        }
    }
}

impl<L: Lang> ConstructParser<L> for EnvironmentBodyParser<'_, L> {
    type Output = EnvironmentBody;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (EnvironmentBody, Option<ParsingStateDelta<L>>)> {
        // The environment-body traceback frame (§3.8), covering the body content *and*
        // the terminator flow — terminator diagnostics name the environment being
        // parsed. Anchored at the invocation trigger.
        let title = match self.invocation_name_span {
            Some(name_span) => FrameTitle::Quoted {
                label: "environment",
                name: SourceSpan::new(&cx.source, name_span),
            },
            None => FrameTitle::Static("environment body"),
        };
        let frame =
            Frame { title, span: SourceSpan::new(&cx.source, self.trigger_span) };
        cx.with_frame(frame, |cx| self.parse_body(cx))
    }
}

impl<L: Lang> EnvironmentBodyParser<'_, L> {
    /// The body parse proper, run under the environment's traceback frame.
    fn parse_body(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (EnvironmentBody, Option<ParsingStateDelta<L>>)> {
        let body_start = cx.tokens.pos();

        // The content loop, stopping at the terminator command — `consume = false`,
        // deliberately: whether the terminator is consumed hinges on the name group
        // *after* it, invisible at the stop point (the §3.6 manual post-stop lookahead).
        // Conditions are tested only at this nesting level, so a terminator inside a
        // nested group or environment never stops this body.
        let stop = StopSpec::at_token(
            TokenStopKind::Command { name: self.stop_command_name },
            false,
        );
        let mut content = NodesParser::new(stop);
        let (outcome, delta) = content.parse(cx)?;
        debug_assert!(delta.is_none(), "NodesParser returns no pass-through delta");

        // The body's content interior: the staged nodes tile it exactly (a stop token's
        // pre-space is already flushed as body content). A last node no one staged (a
        // child construct parser's implementation bug) falls back to the body's start —
        // the builder diagnoses the foreign id when the body list is staged below.
        let body_end = outcome
            .nodes
            .last()
            .and_then(|&last| cx.session.builder.staged_nodes().get(last))
            .map(|node| node.span().end())
            .unwrap_or(body_start);

        let end = match outcome.stop {
            StopCause::TokenCondition { .. } => self.finish_terminator(cx, body_end)?,
            StopCause::EndOfInput => {
                cx.recover(
                    MissingEnvironmentTerminator::new(
                        self.invocation_name,
                        MissingTerminatorFound::EndOfInput,
                    ),
                    SourceSpan::new(&cx.source, self.trigger_span),
                )?;
                body_end
            }
            // A group close nobody at this level asked for: close the body without
            // consuming it (decision 8's unwinding — the stray close is left for an
            // enclosing level, or the root, to claim).
            StopCause::UnexpectedGroupClose { span } => {
                cx.recover(
                    MissingEnvironmentTerminator::new(
                        self.invocation_name,
                        MissingTerminatorFound::StrayGroupClose,
                    ),
                    SourceSpan::new(&cx.source, span),
                )?;
                body_end
            }
            StopCause::NodeCondition => {
                unreachable!("the environment body parser sets no node stop condition")
            }
        };

        let body = cx
            .session
            .builder
            .add(
                NodeKind::list(),
                SourceSpan::new(&cx.source, body_start..body_end),
                Arc::clone(&cx.state),
                outcome.nodes,
            )
            .map_err(|error| cx.implementation_error(error, Span::new(body_start, body_end)))?;
        Ok((EnvironmentBody { body, end }, None))
    }
}

impl<L: Lang> fmt::Debug for EnvironmentBodyParser<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnvironmentBodyParser")
            .field("trigger_span", &self.trigger_span)
            .field("invocation_name", &self.invocation_name)
            .field("stop_command_name", &self.stop_command_name)
            .field("name_group_type", &self.name_group_type)
            .field("match_invocation_name", &self.match_invocation_name)
            .field("invocation_name_span", &self.invocation_name_span)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::super::invocation_parser::parse_declared_arguments;
    use super::super::{
        GroupArgumentParser, Invocation, MarkerArgumentParser, NodesParser,
        OptionalGroupArgumentParser, StopSpec,
    };
    use super::*;
    use crate::engine::{ParseResult, ParserSession};
    use crate::error::{ParseError, Recovery};
    use crate::library::{CallableQuery, CallableSyntax, Library, LibraryStack};
    use crate::node::{
        CallableData, ChildRegion, ContentNodes, NodeRef, ParsedArguments, ParsedSlot,
        ParsedSlots,
    };
    use crate::source::{Source, TextContent};
    use crate::spec::{ArgumentSpec, CallableSpec, StdCallableSpec};
    use crate::state::{
        CommandResolution, ParsingState, ResolvedCallable, StateData, TokenRulesOverrides,
    };
    use crate::token::{
        CommandRule, CommentRule, SpecialsMatch, StdTokenReader, Token, TokenListReader,
        TokenReader, TokenResult, TokenRules, TriggerChars, WhitespaceRules,
    };
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    const GT_BRACE: u32 = 0;
    const GT_OPTION: u32 = 1;
    const GT_RAW: u32 = 2;
    const CT_MACRO: u32 = 10;
    const CT_SPECIALS: u32 = 11;
    const CT_ENVIRONMENT: u32 = 12;

    // --- the test lang: `\`-commands, `{}` groups, `%` comments, `~` specials, and
    // --- `\begin{name}…\end{name}` environments through the composition below ----------

    #[derive(Debug, Clone, Copy)]
    struct EnvLang;
    impl Lang for EnvLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();

        fn resolve_command(
            state: &ParsingState<Self>,
            token: &Token<'_, Self>,
        ) -> CommandResolution<Self> {
            let TokenKind::Command { name, escape_char, .. } = &token.kind else {
                return CommandResolution::Unresolved { detail: None };
            };
            // `\begin` introduces every environment: the shared dispatcher spec's
            // factory returns the composition parser. `\end` deliberately resolves to
            // nothing — inside a body it is the stop condition (checked before the
            // dispatch arms), and an orphan at the root takes the
            // unresolvable-command recovery (§3.6, decision 8).
            if *name == "begin" {
                return CommandResolution::Resolved(ResolvedCallable {
                    callable_type: CT_ENVIRONMENT,
                    spec: Arc::new(BeginSpec),
                });
            }
            let query = CallableQuery::new(
                CT_MACRO,
                name,
                CallableSyntax::Command { escape_char: *escape_char },
            )
            .with_token(token);
            state
                .libraries()
                .resolve(&query, state)
                .map(|spec| ResolvedCallable { callable_type: CT_MACRO, spec })
                .into()
        }

        fn scan_specials<'s>(
            _state: &ParsingState<Self>,
            content: &'s str,
            pos: usize,
        ) -> TokenResult<'s, Self, Option<SpecialsMatch<'s, Self>>> {
            if content[pos..].starts_with('~') {
                Ok(Some(SpecialsMatch {
                    end: pos + 1,
                    callable_type: CT_SPECIALS,
                    name: &content[pos..pos + 1],
                    spec: Arc::new(StdCallableSpec::default()),
                }))
            } else {
                Ok(None)
            }
        }

        fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
            TriggerChars::Only("~".into())
        }
    }

    // --- EnvLang's own conditions: third-party-style derived conditions (the
    // --- extension surface demonstration, §3.8) — plain data structs under the
    // --- language's "envlang." namespace, structurally identical to core conditions
    // --- (`no_constructor`: the tests build them with struct literals) ---------------

    /// `\begin` not followed immediately by its rigid name group.
    #[derive(Debug, Clone, DiagnosticInfo)]
    #[diagnostic(
        id = "envlang.begin.malformed-begin",
        message = "malformed ‘\\begin’: expected its name group immediately",
        no_constructor
    )]
    struct MalformedBegin;

    /// `\begin{name}` naming an environment no library defines.
    #[derive(Debug, Clone, DiagnosticInfo)]
    #[diagnostic(
        id = "envlang.begin.unknown-environment",
        message = "unknown environment ‘{name}’",
        no_constructor
    )]
    struct UnknownEnvironment {
        name: String,
    }

    /// A `\raw` verbatim block missing its `\endraw` terminator.
    #[derive(Debug, Clone, DiagnosticInfo)]
    #[diagnostic(
        id = "envlang.raw.missing-terminator",
        message = "missing ‘\\endraw’ before end of input",
        no_constructor
    )]
    struct MissingRawTerminator;

    // --- the environment-shaped invocation composition (the plan's "test-lang
    // --- EnvironmentSpec analog"): rigid `\begin{name}` scaffolding, environment lookup,
    // --- arguments (the shared 6.5 loop), the body through EnvironmentBodyParser,
    // --- ParsedSlots, empty recorded post-space --------------------------------------

    /// The `\begin` dispatcher: every environment enters through this shared spec.
    #[derive(Debug)]
    struct BeginSpec;

    impl CallableSpec<EnvLang> for BeginSpec {
        /// The honest emptiness answer (slots session, decided direction 2): `\begin`
        /// declares nothing but reads an entire environment — bare use as a
        /// single-token expression argument must be diagnosed, not dispatched (the
        /// deliberate divergence from pylatexenc, which dispatches the environment as
        /// the argument).
        fn requires_content(&self) -> bool {
            true
        }

        fn make_invocation_parser<'a, 's>(
            &'a self,
            invocation: Invocation<'a, 's, EnvLang>,
        ) -> Box<dyn ConstructParser<EnvLang, Output = BuildId> + 'a>
        where
            's: 'a,
        {
            Box::new(EnvironmentInvocationParser { invocation })
        }
    }

    struct EnvironmentInvocationParser<'a, 's> {
        invocation: Invocation<'a, 's, EnvLang>,
    }

    impl ConstructParser<EnvLang> for EnvironmentInvocationParser<'_, '_> {
        type Output = BuildId;

        fn parse(
            &mut self,
            cx: &mut ParseContext<'_, '_, EnvLang>,
        ) -> ConstructParserResult<EnvLang, (BuildId, Option<ParsingStateDelta<EnvLang>>)>
        {
            let trigger = self.invocation.token;

            // Rigid scaffolding: the name group must be the immediately next token.
            let Some(name_group) = read_rigid_name_group(cx, GT_BRACE)? else {
                cx.recover(
                    MalformedBegin,
                    SourceSpan::new(&cx.source, trigger.span),
                )?;
                // Chars fallback over the trigger alone (markup in a Chars node is the
                // accepted tolerant-recovery artifact); nothing past it is consumed.
                let id = cx.session.builder.add(
                    NodeKind::chars(trigger.span),
                    SourceSpan::new(&cx.source, trigger.span),
                    Arc::clone(&cx.state),
                    vec![],
                ).unwrap();
                return Ok((id, None));
            };
            let source = Arc::clone(&cx.source);
            let name = &source.content()[name_group.name_span.range()];

            // Resolve the environment's spec by name.
            let query = CallableQuery::new(CT_ENVIRONMENT, name, CallableSyntax::Other);
            let spec: Arc<dyn CallableSpec<EnvLang>> =
                match cx.state.libraries().resolve(&query, &cx.state) {
                    Some(spec) => spec,
                    None => {
                        cx.recover(
                            UnknownEnvironment { name: name.into() },
                            SourceSpan::new(&cx.source, name_group.name_span),
                        )?;
                        // Tolerant fallback: an argument-less body-only environment,
                        // so the body still parses to its terminator.
                        Arc::new(EnvSpec { arguments: vec![], body_delta: None })
                    }
                };

            // Arguments: the 6.5 machinery, shared with StdInvocationParser. The
            // argument frames quote the *environment's* name, not `\begin`'s.
            let (mut children, arguments) =
                parse_declared_arguments(cx, &spec, name_group.name_span)?;

            // The body: parsed under the slot's state (the body delta stacked on the
            // invocation's base, session-mediated; structural revert) up to and
            // including the rigid `\end{name}` terminator. The delta is the env spec's
            // own field (the A.3 rehoming), read back through the documented
            // Any-supertrait downcast — the core never interprets it; the unknown-
            // environment fallback spec is EnvSpec too, so the downcast only misses
            // for a non-EnvSpec registration (which has no body delta to offer).
            let body_delta = (&*spec as &dyn core::any::Any)
                .downcast_ref::<EnvSpec>()
                .and_then(|env_spec| env_spec.body_delta.clone());
            let slot_state = match &body_delta {
                Some(delta) => cx.session.derived_state(&cx.state, delta),
                None => Arc::clone(&cx.state),
            };
            let mut body_parser =
                EnvironmentBodyParser::new(trigger.span, name, "end", GT_BRACE)
                    .with_invocation_name_span(name_group.name_span);
            let (body, delta) = cx.parse_scoped(slot_state, &mut body_parser)?;
            debug_assert!(delta.is_none());

            let body_children = {
                let staged = cx.session.builder.staged_nodes();
                staged.get(body.body).expect("the body was just staged").children().len()
                    as u32
            };
            let offset = children.len() as u32;
            children.push(body.body);
            // The slot record is pure node vocabulary: minted here, name carried on
            // the record itself (the A.2 reshape).
            let slots = ParsedSlots::from(vec![ParsedSlot::named(
                "body",
                ChildRegion::new(
                    offset..offset + 1,
                    ContentNodes::InChildrenOf(body.body, 0..body_children),
                ),
            )]);

            let data = CallableData {
                callable_type: self.invocation.callable_type,
                // The environment's own name and spec, not the dispatcher's.
                name: name.into(),
                spec,
                arguments: ParsedArguments::from(arguments),
                slots,
                // Environment shapes record empty post-space (§3.5 invariant 3 as
                // amended in 6.4): whitespace after `\begin` is unrecorded scaffolding
                // normalization, whitespace after `\end{…}` is sibling content.
                post_space: TextContent::empty(),
                ext: Default::default(),
            };
            let id = cx.session.builder.add(
                NodeKind::callable(data),
                SourceSpan::new(&cx.source, trigger.span.start()..body.end),
                Arc::clone(&cx.state),
                children,
            ).unwrap();
            Ok((id, None))
        }
    }

    // --- the verbatim takeover (the 6.6 obligation): a parser reading its body as raw
    // --- chars under a features-disabled state, staging an environment-shaped node
    // --- with a slot record — the decided verbatim recipe (§3.2, Action-02 entry) ------

    /// `\raw … \endraw`: the body is raw source bytes — the parser never runs
    /// `NodesParser`, so no terminator-state doctrine is needed (§3.6). It reads the
    /// body through the reader like every other parser (no raw-content peeking): under
    /// the verbatim state every byte is a `Char` token and the terminator one
    /// `GroupClose`.
    #[derive(Debug)]
    struct RawBlockSpec;

    impl CallableSpec<EnvLang> for RawBlockSpec {
        /// The `\verb`-shaped honest emptiness answer: declares nothing, consumes a
        /// raw body — bare expression use must be diagnosed.
        fn requires_content(&self) -> bool {
            true
        }

        fn make_invocation_parser<'a, 's>(
            &'a self,
            invocation: Invocation<'a, 's, EnvLang>,
        ) -> Box<dyn ConstructParser<EnvLang, Output = BuildId> + 'a>
        where
            's: 'a,
        {
            Box::new(RawBlockParser { invocation })
        }
    }

    struct RawBlockParser<'a, 's> {
        invocation: Invocation<'a, 's, EnvLang>,
    }

    impl ConstructParser<EnvLang> for RawBlockParser<'_, '_> {
        type Output = BuildId;

        fn parse(
            &mut self,
            cx: &mut ParseContext<'_, '_, EnvLang>,
        ) -> ConstructParserResult<EnvLang, (BuildId, Option<ParsingStateDelta<EnvLang>>)>
        {
            const TERMINATOR: &str = "\\endraw";
            let trigger = self.invocation.token;
            // The `\verb` idiom — "move_past(trigger, false)" expressed positionally
            // (the uniform `parse` signature cannot tie the stored trigger token to the
            // context's reader): reposition so the trigger's post-space bytes stay raw
            // body content (they are not recorded post-space).
            cx.tokens.move_to_pos(trigger.post_space().start());
            let body_start = cx.tokens.pos();

            // The verbatim state: every feature gate off, and the expected group close
            // *replaced* by the raw terminator. The expected close is ungated by
            // `enable_groups` and overrides any close expectation inherited from an
            // enclosing group, so it is the single recognizer left active: the body
            // arrives as pure `Char` tokens — whitespace and would-be markup included —
            // and the terminator as one `GroupClose`.
            let raw_rule: Arc<GroupRule<EnvLang>> = Arc::new(GroupRule {
                group_type: GT_RAW,
                open: "\\raw".into(),
                close: TERMINATOR.into(),
            });
            let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
                enable_whitespace: Some(false),
                enable_multi_newline_paragraphs: Some(false),
                enable_groups: Some(false),
                enable_commands: Some(false),
                enable_comments: Some(false),
                enable_specials: Some(false),
                expecting_group_close: Some(Some(raw_rule)),
                ..TokenRulesOverrides::default()
            });
            let verbatim_state = cx.session.derived_state(&cx.state, &delta);
            let terminator = cx.with_scoped_state(verbatim_state, |cx| {
                let terminator = loop {
                    let token = cx.tokens.peek(&cx.state).expect("raw body reads as chars");
                    match &token.kind {
                        TokenKind::Char(_) => cx.tokens.move_past(&token, true),
                        TokenKind::GroupClose { .. } | TokenKind::EndOfStream => break token,
                        other => unreachable!("verbatim state yields only chars, got {}", other),
                    }
                };
                cx.tokens.move_past(&terminator, true); // past `\endraw`; no-op at stream end
                terminator
            });
            let (body_end, end) = match &terminator.kind {
                TokenKind::GroupClose { .. } => (terminator.span.start(), terminator.span.end()),
                _ => {
                    cx.recover(
                        MissingRawTerminator,
                        SourceSpan::new(&cx.source, trigger.span),
                    )?;
                    (terminator.span.start(), terminator.span.start())
                }
            };

            let mut body_nodes = Vec::new();
            if body_end > body_start {
                body_nodes.push(cx.session.builder.add(
                    NodeKind::chars(Span::new(body_start, body_end)),
                    SourceSpan::new(&cx.source, body_start..body_end),
                    Arc::clone(&cx.state),
                    vec![],
                ).unwrap());
            }
            let body_children = body_nodes.len() as u32;
            let list = cx.session.builder.add(
                NodeKind::list(),
                SourceSpan::new(&cx.source, body_start..body_end),
                Arc::clone(&cx.state),
                body_nodes,
            ).unwrap();
            // The slot record is minted by the parser — pure node vocabulary, the
            // name carried on the record itself (§3.5 self-description).
            let slots = ParsedSlots::from(vec![ParsedSlot::named(
                "raw",
                ChildRegion::new(0..1, ContentNodes::InChildrenOf(list, 0..body_children)),
            )]);
            let data = CallableData {
                callable_type: self.invocation.callable_type,
                name: self.invocation.name.into(),
                spec: Arc::clone(self.invocation.spec),
                arguments: ParsedArguments::empty(),
                slots,
                post_space: TextContent::empty(),
                ext: Default::default(),
            };
            let id = cx.session.builder.add(
                NodeKind::callable(data),
                SourceSpan::new(&cx.source, trigger.span.start()..end),
                Arc::clone(&cx.state),
                vec![list],
            ).unwrap();
            Ok((id, None))
        }
    }

    // --- states and spec builders ------------------------------------------------------

    fn rules() -> TokenRules<EnvLang> {
        TokenRules {
            enable_whitespace: true,
            whitespace: WhitespaceRules { chars: " \t\n".into() },
            enable_multi_newline_paragraphs: true,
            enable_groups: true,
            groups: vec![Arc::new(GroupRule {
                group_type: GT_BRACE,
                open: "{".into(),
                close: "}".into(),
            })],
            temporary_groups: Vec::new(),
            enable_commands: true,
            commands: vec![Arc::new(CommandRule {
                escape_char: '\\',
                name_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
            })],
            enable_comments: true,
            comments: vec![Arc::new(CommentRule { start: "%".into() })],
            enable_specials: true,
            forbidden_chars: "".into(),
            expecting_group_close: None,
        }
    }

    fn brace_arg() -> Arc<ArgumentSpec<EnvLang>> {
        Arc::new(ArgumentSpec::new(Arc::new(GroupArgumentParser::new(GT_BRACE))))
    }

    fn option_rule() -> Arc<GroupRule<EnvLang>> {
        Arc::new(GroupRule { group_type: GT_OPTION, open: "[".into(), close: "]".into() })
    }

    fn optional_arg() -> Arc<ArgumentSpec<EnvLang>> {
        Arc::new(ArgumentSpec::new(Arc::new(OptionalGroupArgumentParser::new(
            option_rule(),
        ))))
    }

    fn marker_arg(marker: &str) -> Arc<ArgumentSpec<EnvLang>> {
        Arc::new(ArgumentSpec::new(Arc::new(MarkerArgumentParser::new(marker))))
    }

    fn macro_spec(arguments: Vec<Arc<ArgumentSpec<EnvLang>>>) -> Arc<dyn CallableSpec<EnvLang>> {
        Arc::new(StdCallableSpec::new(arguments))
    }

    /// The test-lang environment spec (Phase 7 `EnvironmentSpec` rehearsal): arguments
    /// declared spec-side like any callable; the body's state delta an **ordinary
    /// field** (the A.3 rehoming of pylatexenc's `make_body_parsing_state_delta`),
    /// read back by the composition that drives the parse — the core never sees it.
    #[derive(Debug)]
    struct EnvSpec {
        arguments: Vec<Arc<ArgumentSpec<EnvLang>>>,
        body_delta: Option<ParsingStateDelta<EnvLang>>,
    }

    impl CallableSpec<EnvLang> for EnvSpec {
        fn arguments(&self) -> &[Arc<ArgumentSpec<EnvLang>>] {
            &self.arguments
        }
    }

    fn env_spec(
        arguments: Vec<Arc<ArgumentSpec<EnvLang>>>,
        body_delta: Option<ParsingStateDelta<EnvLang>>,
    ) -> Arc<dyn CallableSpec<EnvLang>> {
        Arc::new(EnvSpec { arguments, body_delta })
    }

    /// The suite's language definitions: the §G matrix — `\frac`, `\emph`, `\alpha`,
    /// `\section*[opt]{title}`, `\item[opt]`, the `\raw` takeover, and the environments
    /// `itemize` (plain), `tabular` (one mandatory argument), `nocomments` (slot delta
    /// disabling comment tokenization in the body), and the plain pair `A`/`B`.
    fn suite_state() -> Arc<ParsingState<EnvLang>> {
        let mut lib = Library::new("test-definitions");
        lib.insert(CT_MACRO, "frac", macro_spec(vec![brace_arg(), brace_arg()]));
        lib.insert(CT_MACRO, "emph", macro_spec(vec![brace_arg()]));
        lib.insert(CT_MACRO, "alpha", macro_spec(vec![]));
        lib.insert(
            CT_MACRO,
            "section",
            macro_spec(vec![marker_arg("*"), optional_arg(), brace_arg()]),
        );
        lib.insert(CT_MACRO, "item", macro_spec(vec![optional_arg()]));
        lib.insert(CT_MACRO, "raw", Arc::new(RawBlockSpec));

        lib.insert(CT_ENVIRONMENT, "itemize", env_spec(vec![], None));
        lib.insert(CT_ENVIRONMENT, "tabular", env_spec(vec![brace_arg()], None));
        lib.insert(
            CT_ENVIRONMENT,
            "nocomments",
            env_spec(
                vec![],
                Some(ParsingStateDelta::new().rules(TokenRulesOverrides {
                    enable_comments: Some(false),
                    ..TokenRulesOverrides::default()
                })),
            ),
        );
        lib.insert(CT_ENVIRONMENT, "A", env_spec(vec![], None));
        lib.insert(CT_ENVIRONMENT, "B", env_spec(vec![], None));

        let mut libraries = LibraryStack::new();
        libraries.push(Arc::new(lib));
        Arc::new(ParsingState::new(StateData { rules: rules(), libraries, mode: (), ext: () }))
    }

    // --- harness (the established driving pattern) --------------------------------------

    struct Parsed {
        result: ParseResult<EnvLang>,
        pos: usize,
    }

    impl fmt::Debug for Parsed {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Parsed")
                .field("shapes", &shapes(&self.result))
                .field("pos", &self.pos)
                .finish()
        }
    }

    /// Drive a root `NodesParser` to end of input, stage the outcome under a root
    /// `List` spanning the parsed extent, freeze, and run the invariant checker.
    fn try_run<'s>(
        content: &'s str,
        tokens: &mut dyn TokenReader<'s, EnvLang>,
        state: &Arc<ParsingState<EnvLang>>,
        recovery: Recovery,
    ) -> Result<Parsed, ParseError> {
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut session = ParserSession::new(recovery);
        let mut cx = ParseContext::new(tokens, Arc::clone(&source), Arc::clone(state), &mut session);
        let mut parser = NodesParser::new(StopSpec::none());
        let (outcome, delta) = parser.parse(&mut cx)?;
        assert_eq!(outcome.stop, StopCause::EndOfInput);
        assert!(delta.is_none());
        let pos = cx.tokens.pos();
        let root_span = {
            let staged = session.builder.staged_nodes();
            match (outcome.nodes.first(), outcome.nodes.last()) {
                (Some(&first), Some(&last)) => Span::new(
                    staged.get(first).unwrap().span().start(),
                    staged.get(last).unwrap().span().end(),
                ),
                _ => Span::empty(0),
            }
        };
        let root = session.builder.add(
            NodeKind::list(),
            SourceSpan::new(&source, root_span),
            Arc::clone(state),
            outcome.nodes,
        ).unwrap();
        let result = session.finish(root).unwrap();
        crate::node::check_tree_invariants(&result.tree);
        Ok(Parsed { result, pos })
    }

    /// [`try_run`] over a `StdTokenReader` — for content whose tokenization is
    /// state-dependent (optional-argument probing, slot deltas, the raw-content
    /// takeover): a pre-scanned token list cannot re-tokenize under momentary states
    /// (`TokenListReader`'s documented fidelity limit).
    fn parse_std(
        content: &str,
        state: &Arc<ParsingState<EnvLang>>,
        recovery: Recovery,
    ) -> Parsed {
        let mut reader = StdTokenReader::new(content);
        try_run(content, &mut reader, state, recovery).expect("parse")
    }

    /// Run against both readers (report R6) and assert they agree; for content whose
    /// tokenization is state-independent.
    fn parse_both(
        content: &str,
        state: &Arc<ParsingState<EnvLang>>,
        recovery: Recovery,
    ) -> Parsed {
        let mut std_reader = StdTokenReader::new(content);
        let a = try_run(content, &mut std_reader, state, recovery).expect("std reader");

        let mut scanned = Vec::new();
        let mut scanner = StdTokenReader::new(content);
        loop {
            let token = TokenReader::next(&mut scanner, state).expect("clean scan");
            let done = matches!(token.kind, TokenKind::EndOfStream);
            scanned.push(token);
            if done {
                break;
            }
        }
        let mut list_reader = TokenListReader::new(scanned);
        let b = try_run(content, &mut list_reader, state, recovery).expect("list reader");

        assert_eq!(shapes(&a.result), shapes(&b.result), "readers disagree on {:?}", content);
        assert_eq!(a.pos, b.pos, "positions disagree on {:?}", content);
        assert_eq!(
            a.result.diagnostics.len(),
            b.result.diagnostics.len(),
            "diagnostics disagree on {:?}",
            content
        );
        assert_eq!(
            a.result.tree.node_count(),
            b.result.tree.node_count(),
            "trees disagree on {:?}",
            content
        );
        a
    }

    /// One readable line per root child: kind, exact span, resolved content.
    fn shapes(result: &ParseResult<EnvLang>) -> Vec<String> {
        result
            .tree
            .root()
            .children()
            .map(|node| {
                let span = format!("{}..{}", node.span().start(), node.span().end());
                match node.kind() {
                    NodeKind::Chars { .. } => {
                        format!("chars {} {:?}", span, node.chars().unwrap())
                    }
                    NodeKind::Comment { .. } => {
                        format!("comment {} {:?}", span, node.comment().unwrap())
                    }
                    NodeKind::Group(_) => format!("group {}", span),
                    NodeKind::Callable(_) => {
                        format!("callable {} {:?}", span, node.name().unwrap())
                    }
                    NodeKind::List { .. } => format!("list {}", span),
                }
            })
            .collect()
    }

    fn root_child(parsed: &Parsed, i: usize) -> NodeRef<'_, EnvLang> {
        parsed.result.tree.root().child(i).expect("root child")
    }

    /// The environment's body nodes (slot 0's content), as (kind-ish string) lines.
    fn body_shapes(env: NodeRef<'_, EnvLang>) -> Vec<String> {
        env.slot_content_nodes(0)
            .expect("an environment has slot 0")
            .map(|node| {
                let span = format!("{}..{}", node.span().start(), node.span().end());
                match node.kind() {
                    NodeKind::Chars { .. } => {
                        format!("chars {} {:?}", span, node.chars().unwrap())
                    }
                    NodeKind::Comment { .. } => {
                        format!("comment {} {:?}", span, node.comment().unwrap())
                    }
                    NodeKind::Group(_) => format!("group {}", span),
                    NodeKind::Callable(_) => {
                        format!("callable {} {:?}", span, node.name().unwrap())
                    }
                    NodeKind::List { .. } => format!("list {}", span),
                }
            })
            .collect()
    }

    fn diagnostics_mentioning(parsed: &Parsed, needle: &str) -> usize {
        parsed
            .result
            .diagnostics
            .iter()
            .filter(|d| d.message().contains(needle))
            .count()
    }

    // --- the environment shape ----------------------------------------------------------

    #[test]
    fn environment_round_trips_with_exact_spans() {
        let st = suite_state();
        //             0....5....1....1....2....2....3....3
        //                  0    5    0    5    0    5
        let content = "x \\begin{itemize} a b \\end{itemize} y";
        let parsed = parse_both(content, &st, Recovery::Strict);

        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..2 \"x \"", "callable 2..35 \"itemize\"", "chars 35..37 \" y\""],
        );
        let env = root_child(&parsed, 1);
        assert_eq!(env.callable_type(), Some(CT_ENVIRONMENT));
        // Environment shapes record empty post-space; the space after `\end{itemize}`
        // is sibling content (§3.5 invariant 3 as amended).
        assert_eq!(env.post_space(), Some(""));
        // Children = the body `List` alone; its span is the body's content interior,
        // whitespace on both ends included.
        assert_eq!(env.child_count(), 1);
        let body = env.slot_content_parent(0).expect("body list");
        assert!(body.is_list());
        assert_eq!(body.span().range(), 17..22);
        assert_eq!(body_shapes(env), ["chars 17..22 \" a b \""]);
        // The records: no arguments, one slot holding the body list — named on the
        // record itself (record-level slots, slots session).
        assert_eq!(env.arguments().unwrap().len(), 0);
        assert_eq!(env.slots().unwrap().len(), 1);
        assert!(env.slots().unwrap().get_named("body").is_some());
        assert!(parsed.result.diagnostics.is_empty());
        assert_eq!(parsed.pos, content.len());
    }

    #[test]
    fn empty_body_is_an_empty_list() {
        let st = suite_state();
        let content = "\\begin{itemize}\\end{itemize}";
        let parsed = parse_both(content, &st, Recovery::Strict);

        let env = root_child(&parsed, 0);
        assert_eq!(env.span().range(), 0..28);
        let body = env.slot_content_parent(0).unwrap();
        assert_eq!(body.span().range(), 15..15);
        assert_eq!(body.child_count(), 0);
        assert_eq!(env.body().unwrap().count(), 0);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn inline_whitespace_in_scaffolding_is_tolerated_and_unrecorded() {
        let st = suite_state();
        //             0....5....1....1....2....2....3
        //                  0    5    0    5    0
        let content = "\\begin {itemize}x\\end {itemize}";
        let parsed = parse_both(content, &st, Recovery::Strict);

        let env = root_child(&parsed, 0);
        assert_eq!(env.span().range(), 0..31);
        assert_eq!(env.name(), Some("itemize"));
        // The commands' post-space is scaffolding normalization: not recorded.
        assert_eq!(env.post_space(), Some(""));
        assert_eq!(body_shapes(env), ["chars 16..17 \"x\""]);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn scaffolding_is_the_reconstructible_complement() {
        let st = suite_state();
        let content = "\\begin{itemize} x \\end{itemize}";
        let parsed = parse_both(content, &st, Recovery::Strict);

        // Node span minus the children block = the `\begin{name}` prefix and the
        // `\end{name}` suffix (decision 6: rigid, reconstructed, not recorded).
        let env = root_child(&parsed, 0);
        let first = env.child(0).unwrap().span().start();
        let last = env.child(env.child_count() - 1).unwrap().span().end();
        assert_eq!(&content[env.span().start()..first], "\\begin{itemize}");
        assert_eq!(&content[last..env.span().end()], "\\end{itemize}");
    }

    #[test]
    fn paragraph_breaks_live_in_the_body() {
        let st = suite_state();
        let content = "\\begin{itemize}a\n\nb\\end{itemize}";
        let parsed = parse_both(content, &st, Recovery::Strict);

        let env = root_child(&parsed, 0);
        assert_eq!(
            body_shapes(env),
            ["chars 15..16 \"a\"", "chars 16..18 \"\\n\\n\"", "chars 18..19 \"b\""],
        );
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn environment_arguments_precede_the_body() {
        let st = suite_state();
        //             0....5....1....1....2....2....3....3
        //                  0    5    0    5    0    5
        let content = "\\begin{tabular}{cc} x \\end{tabular}";
        let parsed = parse_both(content, &st, Recovery::Strict);

        let env = root_child(&parsed, 0);
        assert_eq!(env.span().range(), 0..35);
        // Children: the argument region's group, then the body list.
        assert_eq!(env.child_count(), 2);
        assert!(env.child(0).unwrap().is_group());
        assert!(env.child(1).unwrap().is_list());
        // The argument record designates the group's children as content.
        let content_nodes: Vec<_> =
            env.argument_content_nodes(0).expect("provided").collect();
        assert_eq!(content_nodes.len(), 1);
        assert_eq!(content_nodes[0].chars(), Some("cc"));
        // The slot record holds the body.
        assert_eq!(body_shapes(env), ["chars 19..22 \" x \""]);
        assert!(parsed.result.diagnostics.is_empty());
    }

    // --- nesting and unwinding ----------------------------------------------------------

    #[test]
    fn nested_environments_consume_their_own_terminators() {
        let st = suite_state();
        //             0....5....1....1....2....2....3....3....4
        //                  0    5    0    5    0    5    0
        let content = "\\begin{A} x \\begin{B} y \\end{B} z \\end{A}";
        let parsed = parse_both(content, &st, Recovery::Strict);

        assert_eq!(shapes(&parsed.result), ["callable 0..41 \"A\""]);
        let outer = root_child(&parsed, 0);
        assert_eq!(
            body_shapes(outer),
            ["chars 9..12 \" x \"", "callable 12..31 \"B\"", "chars 31..34 \" z \""],
        );
        let inner = outer.body().unwrap().nth(1).unwrap();
        assert_eq!(body_shapes(inner), ["chars 21..24 \" y \""]);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn same_name_nesting_stays_level_locked() {
        let st = suite_state();
        let content = "\\begin{A}\\begin{A}x\\end{A}\\end{A}";
        let parsed = parse_both(content, &st, Recovery::Strict);

        let outer = root_child(&parsed, 0);
        assert_eq!(outer.span().range(), 0..33);
        let inner = outer.body().unwrap().next().unwrap();
        assert_eq!(inner.span().range(), 9..26);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn mismatched_terminator_unwinds_without_consuming() {
        let st = suite_state();
        //             0....5....1....1....2....2....3
        //                  0    5    0    5    0
        let content = "\\begin{A}x\\begin{B}y\\end{A} z";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        // B's body stops at `\end`, reads the name, sees the mismatch: diagnostic +
        // close WITHOUT consuming — the unwinding lets A find and consume `\end{A}`.
        assert_eq!(shapes(&parsed.result), ["callable 0..27 \"A\"", "chars 27..29 \" z\""]);
        let a = root_child(&parsed, 0);
        assert_eq!(
            body_shapes(a),
            ["chars 9..10 \"x\"", "callable 10..20 \"B\""],
        );
        let b = a.body().unwrap().nth(1).unwrap();
        // B's extent ends at its body's end, before the terminator it did not consume.
        assert_eq!(b.span().range(), 10..20);
        assert_eq!(body_shapes(b), ["chars 19..20 \"y\""]);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(diagnostics_mentioning(&parsed, "‘B’"), 1);
    }

    #[test]
    fn mismatch_diagnostic_carries_the_nested_environment_frames() {
        // B's mismatch is diagnosed under B's body frame (innermost), B's `\begin`
        // invocation frame, A's body frame, and A's `\begin` invocation frame — the
        // pylatexenc-style "while parsing" traceback, from a single attachment point.
        let st = suite_state();
        let content = "\\begin{A}x\\begin{B}y\\end{A} z";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        let diagnostic = parsed
            .result
            .diagnostics
            .with_identifier(EnvironmentTerminatorMismatch::IDENTIFIER)
            .next()
            .unwrap();
        let condition =
            diagnostic.data().downcast_ref::<EnvironmentTerminatorMismatch>().unwrap();
        assert_eq!(condition.expected, "B");
        assert_eq!(condition.found, "A");
        let titles: Vec<&str> = diagnostic.frames().iter().map(|f| f.title()).collect();
        assert_eq!(
            titles,
            [
                "environment ‘B’",
                "callable ‘\\begin’",
                "environment ‘A’",
                "callable ‘\\begin’",
            ]
        );
    }

    #[test]
    fn orphan_terminator_takes_unresolvable_command_recovery() {
        let st = suite_state();
        let content = "x\\end{a}y";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        // The orphan `\end` reaches the root loop as an ordinary command: diagnostic +
        // chars fallback (not merged into neighboring runs); `{a}` is a plain group.
        assert_eq!(
            shapes(&parsed.result),
            [
                "chars 0..1 \"x\"",
                "chars 1..5 \"\\\\end\"",
                "group 5..8",
                "chars 8..9 \"y\"",
            ],
        );
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(diagnostics_mentioning(&parsed, "cannot resolve command"), 1);
    }

    #[test]
    fn terminator_inside_a_nested_group_does_not_stop_the_body() {
        let st = suite_state();
        //             0....5....1....1....2....2....3
        //                  0    5    0    5    0
        let content = "\\begin{A}{\\end{A}}\\end{A}";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        // Stop conditions are tested only at the body's own nesting level: inside the
        // brace group, `\end` is an ordinary (unresolvable) command — diagnosed chars
        // fallback + a plain `{A}` group — and the environment terminates at the outer
        // `\end{A}`.
        let env = root_child(&parsed, 0);
        assert_eq!(env.span().range(), 0..25);
        assert_eq!(body_shapes(env), ["group 9..18"]);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(diagnostics_mentioning(&parsed, "cannot resolve command"), 1);
    }

    #[test]
    fn environment_in_a_group_and_group_in_a_body() {
        let st = suite_state();
        //             0....5....1....1....2....2....3
        //                  0    5    0    5    0
        let content = "{\\begin{A} {x} \\end{A}}";
        let parsed = parse_both(content, &st, Recovery::Strict);

        assert_eq!(shapes(&parsed.result), ["group 0..23"]);
        let env = root_child(&parsed, 0).child(0).unwrap();
        assert!(env.is_callable());
        assert_eq!(env.span().range(), 1..22);
        assert_eq!(
            body_shapes(env),
            ["chars 10..11 \" \"", "group 11..14", "chars 14..15 \" \""],
        );
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn stray_group_close_in_the_body_unwinds() {
        let st = suite_state();
        //             0....5....1....1....2
        //                  0    5    0
        let content = "{\\begin{A} x } y";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        // The body's loop reports the `}` as data; the environment closes without
        // consuming it (diagnostic), and the enclosing group then claims its close.
        assert_eq!(shapes(&parsed.result), ["group 0..14", "chars 14..16 \" y\""]);
        let env = root_child(&parsed, 0).child(0).unwrap();
        assert!(env.is_callable());
        assert_eq!(env.span().range(), 1..13);
        assert_eq!(body_shapes(env), ["chars 10..13 \" x \""]);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(diagnostics_mentioning(&parsed, "missing terminator"), 1);
    }

    // --- terminator recovery -------------------------------------------------------------

    #[test]
    fn missing_terminator_at_end_of_input() {
        let st = suite_state();
        let content = "\\begin{itemize} x";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        let env = root_child(&parsed, 0);
        assert_eq!(env.span().range(), 0..17);
        assert_eq!(body_shapes(env), ["chars 15..17 \" x\""]);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(diagnostics_mentioning(&parsed, "before end of input"), 1);
        assert_eq!(parsed.pos, content.len());
    }

    #[test]
    fn unterminated_environments_unwind_at_end_of_input() {
        let st = suite_state();
        let content = "\\begin{A}\\begin{B}x";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        // Both levels diagnose their own missing terminator (loop safety at EOF).
        let a = root_child(&parsed, 0);
        assert_eq!(a.span().range(), 0..19);
        let b = a.body().unwrap().next().unwrap();
        assert_eq!(b.span().range(), 9..19);
        assert_eq!(parsed.result.diagnostics.len(), 2);
        assert_eq!(diagnostics_mentioning(&parsed, "before end of input"), 2);
    }

    #[test]
    fn malformed_terminator_consumes_the_command_alone() {
        let st = suite_state();
        //             0....5....1....1....2
        //                  0    5    0
        let content = "\\begin{A}x\\end[y]";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        // `\end` not followed by its name group: diagnosed, consumed, the environment
        // closes — and `[y]` re-parses as ordinary sibling content (no option rule is
        // in scope at the root: plain chars).
        assert_eq!(
            shapes(&parsed.result),
            ["callable 0..14 \"A\"", "chars 14..17 \"[y]\""],
        );
        let env = root_child(&parsed, 0);
        assert_eq!(body_shapes(env), ["chars 9..10 \"x\""]);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(diagnostics_mentioning(&parsed, "malformed terminator"), 1);
    }

    #[test]
    fn malformed_terminator_at_end_of_input() {
        let st = suite_state();
        let content = "\\begin{A}x\\end";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        let env = root_child(&parsed, 0);
        assert_eq!(env.span().range(), 0..14);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(diagnostics_mentioning(&parsed, "malformed terminator"), 1);
    }

    #[test]
    fn whitespace_in_the_terminator_name_is_malformed() {
        let st = suite_state();
        //             0....5....1....1....2....2
        //                  0    5    0
        let content = "\\begin{A}x\\end{ A }";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        // Rigid syntax: no whitespace inside the name group. The command is consumed,
        // the group re-parses as sibling content.
        assert_eq!(
            shapes(&parsed.result),
            ["callable 0..14 \"A\"", "group 14..19"],
        );
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(diagnostics_mentioning(&parsed, "malformed terminator"), 1);
    }

    #[test]
    fn strict_mode_aborts_on_terminator_problems() {
        let st = suite_state();
        for content in ["\\begin{A}x\\end{B}", "\\begin{A}x", "\\begin{A}x\\end[y]"] {
            let mut reader = StdTokenReader::new(content);
            let result = try_run(content, &mut reader, &st, Recovery::Strict);
            assert!(result.is_err(), "expected strict abort on {:?}", content);
        }
    }

    // --- the begin side -------------------------------------------------------------------

    #[test]
    fn malformed_begin_falls_back_to_chars() {
        let st = suite_state();
        let content = "\\begin x";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        // The fallback spans the trigger alone (post-space included — the token span
        // convention); nothing past it is consumed, and runs do not merge into it.
        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..7 \"\\\\begin \"", "chars 7..8 \"x\""],
        );
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(diagnostics_mentioning(&parsed, "malformed ‘\\begin’"), 1);
    }

    #[test]
    fn whitespace_in_the_begin_name_is_malformed() {
        let st = suite_state();
        let content = "\\begin{x y}z";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        assert_eq!(
            shapes(&parsed.result),
            ["chars 0..6 \"\\\\begin\"", "group 6..11", "chars 11..12 \"z\""],
        );
        assert_eq!(parsed.result.diagnostics.len(), 1);
    }

    #[test]
    fn unknown_environment_still_parses_its_body() {
        let st = suite_state();
        let content = "\\begin{nope} x \\end{nope}";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        let env = root_child(&parsed, 0);
        assert_eq!(env.name(), Some("nope"));
        assert_eq!(env.span().range(), 0..25);
        assert_eq!(body_shapes(env), ["chars 12..15 \" x \""]);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(diagnostics_mentioning(&parsed, "unknown environment"), 1);
    }

    // --- slot state deltas ----------------------------------------------------------------

    #[test]
    fn slot_state_delta_scopes_the_body() {
        let st = suite_state();
        //             0....5....1....1....2....2....3....3....4....4
        //                  0    5    0    5    0    5    0    5
        let content = "\\begin{nocomments} a%b \\end{nocomments} %c";
        let parsed = parse_std(content, &st, Recovery::Strict);

        // Inside the body the slot's delta disables comment tokenization: `%b` is
        // chars. The terminator is still found under the slot state, and after the
        // structural revert `%c` is a comment again.
        let env = root_child(&parsed, 0);
        assert_eq!(body_shapes(env), ["chars 18..23 \" a%b \""]);
        assert!(root_child(&parsed, 2).is_comment());
        assert!(parsed.result.diagnostics.is_empty());
    }

    // --- the match_invocation_name switch --------------------------------------------------

    #[test]
    fn match_invocation_name_can_be_disabled() {
        // Driven directly (the composition above always back-references): with the
        // switch off, any rigid name group closes the body, without a diagnostic.
        let content = "x\\end{whatever} rest";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let st = suite_state();
        let mut reader = StdTokenReader::new(content);
        let mut session = ParserSession::new(Recovery::Strict);
        let mut cx = ParseContext::new(&mut reader, Arc::clone(&source), Arc::clone(&st), &mut session);
        let mut parser = EnvironmentBodyParser::new(Span::empty(0), "A", "end", GT_BRACE)
            .with_match_invocation_name(false);
        let (body, delta) = parser.parse(&mut cx).expect("parse");
        assert!(delta.is_none());
        assert_eq!(body.end, 15);
        assert_eq!(cx.tokens.pos(), 15);

        let result = session.finish(body.body).unwrap();
        crate::node::check_tree_invariants(&result.tree);
        assert_eq!(result.tree.root().span().range(), 0..1);
        assert_eq!(result.tree.root().child(0).unwrap().chars(), Some("x"));
        assert!(result.diagnostics.is_empty());
    }

    // --- the verbatim escape hatch ----------------------------------------------------------

    #[test]
    fn verbatim_body_via_the_takeover_hatch() {
        let st = suite_state();
        //             0....5....1....1....2....2....3
        //                  0    5    0    5    0
        let content = "\\raw {\\begin %~\\endraw z";
        let parsed = parse_std(content, &st, Recovery::Strict);

        // The takeover reads raw source bytes — markup, comment chars, specials and
        // all — into one Chars body node; the trigger's post-space stays raw content
        // (the `\verb` reposition idiom), and the slot record is parser-minted.
        assert_eq!(
            shapes(&parsed.result),
            ["callable 0..22 \"raw\"", "chars 22..24 \" z\""],
        );
        let raw = root_child(&parsed, 0);
        assert_eq!(raw.post_space(), Some(""));
        assert_eq!(raw.slots().unwrap().len(), 1);
        assert!(raw.slots().unwrap().get_named("raw").is_some());
        assert_eq!(body_shapes(raw), ["chars 4..15 \" {\\\\begin %~\""]);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn verbatim_body_inside_a_group_overrides_the_inherited_close() {
        // The Action-02 pitfall: inside a braces group the interior state expects `}`
        // (ungated by `enable_groups`), so a verbatim state that merely disabled the
        // feature gates would still read the body's `}` as a GroupClose. The recipe
        // *replaces* the expectation with the raw terminator instead: the `}` stays
        // body content, and the enclosing group still finds its own close afterwards.
        let st = suite_state();
        //             0....5....1....1....
        //                  0    5
        let content = "{a\\raw }b\\endraw c}";
        let parsed = parse_std(content, &st, Recovery::Strict);

        let group = root_child(&parsed, 0);
        assert!(group.is_group());
        assert_eq!(group.span().range(), 0..19);
        let raw = group.child(1).unwrap();
        assert_eq!(raw.span().range(), 2..16);
        assert_eq!(body_shapes(raw), ["chars 6..9 \" }b\""]);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn verbatim_body_missing_its_terminator() {
        let st = suite_state();
        let content = "\\raw abc";
        let parsed = parse_std(content, &st, Recovery::Tolerant);

        let raw = root_child(&parsed, 0);
        assert_eq!(raw.span().range(), 0..8);
        assert_eq!(body_shapes(raw), ["chars 4..8 \" abc\""]);
        assert_eq!(parsed.result.diagnostics.len(), 1);
        assert_eq!(diagnostics_mentioning(&parsed, "\\endraw"), 1);
    }

    // --- the requires_content() override (slots session): `\begin` bare in expression
    // --- position is diagnosed, not dispatched ----------------------------------------

    /// The documented divergence from pylatexenc (which dispatches the environment as
    /// the argument): `\emph\begin{A}…` diagnoses the bare `\begin` — `BeginSpec`
    /// declares nothing but overrides `requires_content()` — and stages it alone as
    /// `\emph`'s argument. The environment's pieces stay sibling content, its orphan
    /// `\end` taking the unresolvable-command recovery.
    #[test]
    fn bare_begin_in_expression_position_is_diagnosed() {
        let st = suite_state();
        //             0....5....1....1....2..
        //                  0    5    0
        let content = "\\emph\\begin{A}x\\end{A}";
        let parsed = parse_both(content, &st, Recovery::Tolerant);

        assert_eq!(
            shapes(&parsed.result),
            [
                "callable 0..11 \"emph\"",
                "group 11..14",
                "chars 14..15 \"x\"",
                "chars 15..19 \"\\\\end\"",
                "group 19..22",
            ],
        );
        let emph = root_child(&parsed, 0);
        let bare = emph.argument_content_nodes(0).unwrap().next().unwrap();
        assert_eq!(bare.name(), Some("begin"));
        assert_eq!(bare.child_count(), 0);
        assert_eq!(parsed.result.diagnostics.len(), 2);
        assert_eq!(
            parsed.result.diagnostics.iter().next().unwrap().identifier(),
            crate::constructs::ExpressionCallableRequiresContent::IDENTIFIER,
        );
    }

    // --- the end-to-end §G suite ------------------------------------------------------------

    #[test]
    fn end_to_end_realistic_snippet() {
        let st = suite_state();
        let content = "Intro \\emph{te}~xt. % note\n\\begin{itemize} \\item[a] one\n\n\\item two \\frac{1}{2}\n\\end{itemize} tail \\section*{Top}!";
        let parsed = parse_std(content, &st, Recovery::Strict);

        assert_eq!(
            shapes(&parsed.result),
            [
                "chars 0..6 \"Intro \"",
                "callable 6..15 \"emph\"",
                "callable 15..16 \"~\"",
                "chars 16..20 \"xt. \"",
                "comment 20..27 \" note\"",
                "callable 27..92 \"itemize\"",
                "chars 92..98 \" tail \"",
                "callable 98..112 \"section\"",
                "chars 112..113 \"!\"",
            ],
        );

        // Post-space placement: `\emph` and `\section` have none (delimiters follow
        // directly); the specials `~` records none either.
        assert_eq!(root_child(&parsed, 1).post_space(), Some(""));
        assert_eq!(root_child(&parsed, 7).post_space(), Some(""));

        // The environment's body: whitespace, two \item invocations (one with its
        // option, one bare — the bare one's span includes its own syntactic
        // post-space), a paragraph break, and the \frac.
        let env = root_child(&parsed, 5);
        assert_eq!(env.callable_type(), Some(CT_ENVIRONMENT));
        assert_eq!(
            body_shapes(env),
            [
                "chars 42..43 \" \"",
                "callable 43..51 \"item\"",
                "chars 51..55 \" one\"",
                "chars 55..57 \"\\n\\n\"",
                "callable 57..63 \"item\"",
                "chars 63..67 \"two \"",
                "callable 67..78 \"frac\"",
                "chars 78..79 \"\\n\"",
            ],
        );
        let body: Vec<_> = env.body().unwrap().collect();
        let item_with_option = body[1];
        assert!(item_with_option.arguments().unwrap().get(0).unwrap().is_provided());
        let bare_item = body[4];
        assert!(!bare_item.arguments().unwrap().get(0).unwrap().is_provided());
        let frac = body[6];
        let ones: Vec<_> = frac.argument_content_nodes(0).unwrap().collect();
        assert_eq!(ones[0].chars(), Some("1"));

        // `\section*{Top}`: marker provided (a Chars content node), option absent,
        // title group provided.
        let section = root_child(&parsed, 7);
        let args = section.arguments().unwrap();
        assert!(args.get(0).unwrap().is_provided());
        assert!(!args.get(1).unwrap().is_provided());
        let title: Vec<_> = section.argument_content_nodes(2).unwrap().collect();
        assert_eq!(title[0].chars(), Some("Top"));

        assert!(parsed.result.diagnostics.is_empty());
        assert_eq!(parsed.pos, content.len());
    }
}
