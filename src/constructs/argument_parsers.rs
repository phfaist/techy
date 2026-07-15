//! The standard [`ArgumentParser`] implementations (Phase 6.5) and the shared
//! noise-scan helper: [`GroupArgumentParser`] (mandatory delimited group with the
//! single-expression fallback), [`OptionalGroupArgumentParser`] (optional group whose
//! delimiters are minted for the occasion), [`MarkerArgumentParser`] (literal markers
//! like `*`), and [`ExpressionParser`] (one node: group / full invocation / single
//! char) — pylatexenc's `'{'` / `'['` / `'*'` argument shorthands resolved into core
//! parsers, parameterized by group types and rules (no privileged spellings, §2.3;
//! preset one-liner constructors are Phase 7).
//!
//! # Regions, noise, and the absent contract (DESIGN_RATIONALE.md §3.5)
//!
//! An argument parser owns its argument's **entire region**, leading noise included: it
//! scans whitespace and comments itself ([`scan_argument_noise`]) and stages them as
//! ordinary nodes ahead of the argument's syntax, under the argument's own state — the
//! caller ([`StdInvocationParser`](super::StdInvocationParser)) has already stacked the
//! [`ArgumentSpec::parsing_state_delta`] on the invocation's base state, so noise
//! policy runs under the argument's rules. Content is designated at parse time
//! ([`ContentNodes`]): the parser knows whether a group's braces are argument syntax
//! (content = the group's children) or the node itself is the value (a `\frac 1 2`
//! single token, a `*` marker — both **count as content**, pylatexenc parity).
//!
//! **Absent means nothing was consumed**: the reader is rewound to where the scan
//! started (probed noise is re-parsed as enclosing content — an absent-optional probe
//! before a present mandatory re-scans the same noise, by design) and speculatively
//! staged nodes are never claimed (the builder drops them).
//!
//! # Recovery (DESIGN_RATIONALE.md §3.8, detection-site rules)
//!
//! A missing *mandatory* argument is diagnosed here — tolerant: diagnostic + report
//! absent; strict: abort — while a missing optional or marker is silent. An
//! unresolvable command in expression position takes the loop's chars-fallback
//! recovery. A tokenizer error encountered while probing is **not** consumed and not
//! diagnosed here: the argument is reported absent and the enclosing content loop
//! re-reads the error and applies its own token recovery (diagnosing it here too would
//! double-report); under strict mode the probe aborts with the token error, exactly as
//! the loop would.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::mem;

use crate::error::DiagnosticInfo;
use crate::node::{
    BuildId, CallableData, ContentNodes, NodeKind, ParsedArgument, ParsedArguments,
    ParsedSlots,
};
use crate::source::{SourceSpan, Span, TextContent};
use crate::spec::{ArgumentParser, ArgumentSpec, ParsedArgumentNodes};
use crate::state::{Lang, ParsingState, ParsingStateDelta, TokenRulesOverrides};
use crate::token::{GroupRule, Token, TokenKind};

use super::child_state::{ChildStateSpec, GroupChildState, InvocationChildState};
use super::group_parser::GroupParser;
use super::nodes_parser::{ExpressionCallableRequiresContent, UnresolvableCommand};
use super::{
    ConstructParser, ConstructParserResult, Invocation, ParseContext,
};

/// Condition: a mandatory argument was missing at its position (end of input, a
/// paragraph break, an enclosing group close) — detected by the mandatory argument
/// parsers, which report the argument absent after recording it
/// (DESIGN_RATIONALE.md §3.8). No callable name in the payload: the frame stack renders
/// the enclosing invocation.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.argument_parsers.missing-mandatory-argument")]
pub struct MissingMandatoryArgument {
    /// The argument's declared name, when the spec has one.
    pub argument_name: Option<String>,
}

// Hand-written wording: the name is appended only when the spec declares one (a
// conditional, which the message format string cannot express).
impl fmt::Display for MissingMandatoryArgument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "missing mandatory argument")?;
        if let Some(name) = &self.argument_name {
            write!(f, " ‘{}’", name)?;
        }
        Ok(())
    }
}

/// Condition: no expression could start at a mandatory single-expression argument's
/// position ([`ExpressionParser`]).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.argument_parsers.expected-expression-argument")]
pub struct ExpectedExpressionArgument {
    /// The argument's declared name, when the spec has one.
    pub argument_name: Option<String>,
}

// Hand-written wording: same conditional shape as [`MissingMandatoryArgument`].
impl fmt::Display for ExpectedExpressionArgument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expected an expression argument")?;
        if let Some(name) = &self.argument_name {
            write!(f, " ‘{}’", name)?;
        }
        Ok(())
    }
}

/// The argument's declared name as an owned payload field
/// ([`MissingMandatoryArgument::argument_name`]).
fn argument_name<L: Lang>(spec: &ArgumentSpec<L>) -> Option<String> {
    spec.name.as_deref().map(String::from)
}

// --- the shared noise scan ----------------------------------------------------------

/// The result of [`scan_argument_noise`]: staged leading-noise nodes, the rewind
/// target, and the first non-noise token.
pub struct ArgumentNoise<'s, L: Lang> {
    /// The staged noise nodes (comment nodes, whitespace-only `Chars` nodes), in source
    /// order — the leading part of the argument's region if the argument turns out
    /// present; unclaimed (and dropped by the builder) otherwise.
    pub nodes: Vec<BuildId>,
    /// The reader position before the scan: where [`rewind`](ArgumentNoise::rewind)
    /// returns to when the argument is absent.
    pub start: usize,
    /// The first non-noise token, peeked and left unconsumed; its `pre_space` is *not*
    /// staged (the parser stages it via [`stage_pre_space`] once it commits to the
    /// argument being present). `None` when a tokenizer error sits at the position
    /// (tolerant mode — see the module docs on recovery).
    pub next: Option<Token<'s, L>>,
}

impl<L: Lang> ArgumentNoise<'_, L> {
    /// Report the argument absent: reposition the reader to where the scan started, so
    /// the probed noise is re-parsed as enclosing content. The staged noise nodes are
    /// simply never claimed — the builder drops them.
    pub fn rewind(&self, cx: &mut ParseContext<'_, '_, L>) {
        cx.tokens.move_to_pos(self.start);
    }
}

impl<L: Lang> fmt::Debug for ArgumentNoise<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArgumentNoise")
            .field("nodes", &self.nodes)
            .field("start", &self.start)
            .field("next", &self.next)
            .finish()
    }
}

/// Scan an argument's leading noise — whitespace and comments ahead of its syntax —
/// staging each as an ordinary node under the context's current state (the argument's
/// own state: noise policy is inseparable from argument syntax, DESIGN_RATIONALE.md
/// §3.5), and stop at the first non-noise token, peeked and left unconsumed.
///
/// The shared entry step of the standard argument parsers; custom [`ArgumentParser`]s
/// with ordinary noise behavior use it the same way. Parsers whose syntax involves the
/// whitespace or comment characters (verbatim-delimited arguments) skip it and read
/// raw tokens instead — the scan is deliberately *not* run by the invocation parser on
/// a parser's behalf.
pub fn scan_argument_noise<'s, L: Lang>(
    cx: &mut ParseContext<'_, 's, L>,
) -> ConstructParserResult<L, ArgumentNoise<'s, L>> {
    let start = cx.tokens.pos();
    let mut nodes = Vec::new();
    let state = Arc::clone(&cx.state); // staging noise nodes never changes the state
    loop {
        let Some(token) = cx.probe_token(&state)? else {
            return Ok(ArgumentNoise { nodes, start, next: None });
        };
        match &token.kind {
            TokenKind::Comment { start, post_space, .. } => {
                stage_pre_space(cx, &mut nodes, token.pre_space)?;
                // The token's sub-spans tile its span: start delimiter, content,
                // post-space.
                let content_span = Span::new(start.end(), post_space.start());
                let kind = NodeKind::comment(*start, content_span, *post_space);
                nodes.push(stage(cx, kind, token.span)?);
                cx.tokens.move_past(&token, true);
            }
            _ => return Ok(ArgumentNoise { nodes, start, next: Some(token) }),
        }
    }
}

/// Stage `pre_space` as a whitespace-only `Chars` node (if non-empty) and record it in
/// `nodes` — how a committed token's pre-space becomes the region's leading noise
/// (whitespace before an argument is a node like everywhere else, §3.5).
pub fn stage_pre_space<L: Lang>(
    cx: &mut ParseContext<'_, '_, L>,
    nodes: &mut Vec<BuildId>,
    pre_space: Span,
) -> ConstructParserResult<L, ()> {
    if !pre_space.is_empty() {
        nodes.push(stage(cx, NodeKind::chars(pre_space), pre_space)?);
    }
    Ok(())
}

/// Stage a childless node under the context's current state.
fn stage<L: Lang>(
    cx: &mut ParseContext<'_, '_, L>,
    kind: NodeKind<L>,
    span: Span,
) -> ConstructParserResult<L, BuildId> {
    cx.session
        .builder
        .add(kind, SourceSpan::new(&cx.source, span), Arc::clone(&cx.state), Vec::new())
        .map_err(|error| cx.implementation_error(error, span))
}

// --- the expression core --------------------------------------------------------------

/// Parse the single expression node starting at `next` (a noise scan's stop token,
/// unconsumed): a delimited group of any class in scope (consumed whole), a full
/// callable invocation, or a single content char — staging `next`'s pre-space into
/// `nodes` first. Returns `Ok(None)` — nothing consumed, nothing staged — when `next`
/// cannot begin an expression (end of input, a paragraph break, a group close: the
/// enclosing structure's business).
///
/// Invocations dispatch through the spec's full `make_invocation_parser` factory path
/// (takeover parsers included) under the current state — the descent policy question
/// does not reach here ([`ChildStateSpec`](super::ChildStateSpec) is one level deep,
/// §3.6) — with two deliberate rules:
///
/// - A callable whose invocation **requires content**
///   ([`CallableSpec::requires_content`](crate::spec::CallableSpec::requires_content):
///   some declared argument cannot match empty, or a body-bearing takeover spec
///   overrides it) cannot be *used* bare as a single-token expression (pylatexenc's
///   requires-arguments diagnostic). Tolerant recovery stages the **bare single-token
///   callable** — the trigger alone, every declared argument absent, no slots —
///   consuming nothing beyond the trigger: exactly the single token the expression
///   position asked for, so a `\frac\sqrt2`-shaped source leaves `2` for `\frac`'s
///   next argument rather than letting `\sqrt` swallow it. A callable all of whose
///   arguments can match empty (`\mymacro` taking one optional argument) dispatches in
///   full — pylatexenc parity.
/// - An after-effect delta returned by the invocation parser is **dropped**: an
///   argument scopes no state beyond its own extent, and
///   [`ArgumentParser::parse_argument`] deliberately has no delta channel.
fn parse_expression_node<'s, L: Lang>(
    cx: &mut ParseContext<'_, 's, L>,
    next: &Token<'s, L>,
    nodes: &mut Vec<BuildId>,
) -> ConstructParserResult<L, Option<BuildId>> {
    match &next.kind {
        TokenKind::Char(_) => {
            stage_pre_space(cx, nodes, next.pre_space)?;
            cx.tokens.move_past(next, true);
            let id = stage(cx, NodeKind::chars(next.span), next.span)?;
            nodes.push(id);
            Ok(Some(id))
        }

        TokenKind::GroupOpen { rule, .. } => {
            stage_pre_space(cx, nodes, next.pre_space)?;
            let rule = Arc::clone(rule);
            cx.tokens.move_past(next, true);
            let mut group = GroupParser::new(next.span, rule);
            let (id, _delta) = group.parse(cx)?; // groups have no after-effect
            nodes.push(id);
            Ok(Some(id))
        }

        TokenKind::Command { name, escape_char, .. } => {
            // Resolution under the current state, coherent with the state that
            // tokenized the token (§3.6).
            match L::resolve_command(&cx.state, next) {
                Some(resolved) => {
                    let invocation = Invocation {
                        callable_type: resolved.callable_type,
                        name,
                        spec: &resolved.spec,
                        token: next,
                    };
                    dispatch_expression_invocation(cx, nodes, invocation)
                }
                None => {
                    // The decided unresolvable-command recovery (§3.8), in expression
                    // position: diagnostic + span-backed chars fallback, the token
                    // consumed whole — mirroring the content loop.
                    cx.recover(
                        UnresolvableCommand::new(*name, *escape_char),
                        SourceSpan::new(&cx.source, next.span),
                    )?;
                    stage_pre_space(cx, nodes, next.pre_space)?;
                    cx.tokens.move_past(next, true);
                    let id = stage(cx, NodeKind::chars(next.span), next.span)?;
                    nodes.push(id);
                    Ok(Some(id))
                }
            }
        }

        TokenKind::Specials { callable_type, name, spec } => {
            // Recognition = resolution: the token carries the full resolution.
            let invocation =
                Invocation { callable_type: *callable_type, name, spec, token: next };
            dispatch_expression_invocation(cx, nodes, invocation)
        }

        // No expression starts here; the caller decides what absence means.
        TokenKind::ParagraphBreak
        | TokenKind::GroupClose { .. }
        | TokenKind::EndOfStream
        | TokenKind::Comment { .. } => Ok(None),
    }
}

/// The invocation half of [`parse_expression_node`] (see its docs for the two rules).
fn dispatch_expression_invocation<'s, L: Lang>(
    cx: &mut ParseContext<'_, 's, L>,
    nodes: &mut Vec<BuildId>,
    invocation: Invocation<'_, 's, L>,
) -> ConstructParserResult<L, Option<BuildId>> {
    let token = invocation.token;
    if invocation.spec.requires_content() {
        // The trigger's written spelling, built only on this cold branch (the hot
        // dispatch path stays allocation-free).
        let spelling = match &token.kind {
            TokenKind::Command { name, escape_char, .. } => {
                format!("{}{}", escape_char, name)
            }
            _ => invocation.name.into(),
        };
        cx.recover(
            ExpressionCallableRequiresContent::new(spelling),
            SourceSpan::new(&cx.source, token.span),
        )?;
        stage_pre_space(cx, nodes, token.pre_space)?;
        cx.tokens.move_past(token, true);
        // The bare single-token callable: every declared argument absent, no slots —
        // the record stays self-describing (each entry keeps its spec).
        let arguments: Vec<ParsedArgument<L>> = invocation
            .spec
            .arguments()
            .iter()
            .map(|argument_spec| ParsedArgument::absent(Arc::clone(argument_spec)))
            .collect();
        let data = CallableData {
            callable_type: invocation.callable_type,
            name: invocation.name.into(),
            spec: Arc::clone(invocation.spec),
            arguments: ParsedArguments::from(arguments),
            slots: ParsedSlots::empty(),
            post_space: TextContent::Spanned(token.post_space()),
            ext: Default::default(),
        };
        let id = cx
            .session
            .builder
            .add(
                NodeKind::callable(data),
                SourceSpan::new(&cx.source, token.span),
                Arc::clone(&cx.state),
                Vec::new(),
            )
            .map_err(|error| cx.implementation_error(error, token.span))?;
        nodes.push(id);
        return Ok(Some(id));
    }

    stage_pre_space(cx, nodes, token.pre_space)?;
    // The invocation's traceback frame (the expression-position dispatch site, §3.8).
    let frame = super::invocation_frame(cx, &invocation);
    // Consume the trigger whole before the parser runs (the dispatch contract, §3.6).
    cx.tokens.move_past(token, true);
    let spec = invocation.spec;
    let mut parser = spec.make_invocation_parser(invocation);
    let result = cx.with_frame(frame, |cx| parser.parse(cx));
    drop(parser);
    let (id, _delta) = result?; // after-effect deltas have no scope here (fn docs)
    nodes.push(id);
    Ok(Some(id))
}

// --- the standard argument parsers ----------------------------------------------------

/// The single-expression argument parser (pylatexenc's `LatexExpressionParser`): the
/// argument is **one node** — a delimited group of any class in scope, a full callable
/// invocation, or a single content char. The expression node itself is the content
/// (its delimiters, where it has any, are part of the value — contrast
/// [`GroupArgumentParser`], whose matched delimiters are argument syntax).
///
/// The expression is mandatory: when none can start at the position, the parser
/// diagnoses (tolerant) or aborts (strict) and reports the argument absent, consuming
/// nothing. Also the fallback engine of [`GroupArgumentParser`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ExpressionParser;

impl ExpressionParser {
    /// The single-expression argument parser.
    pub fn new() -> ExpressionParser {
        ExpressionParser
    }
}

impl<L: Lang> ArgumentParser<L> for ExpressionParser {
    fn parse_argument(
        &self,
        cx: &mut ParseContext<'_, '_, L>,
        spec: &ArgumentSpec<L>,
    ) -> ConstructParserResult<L, Option<ParsedArgumentNodes>> {
        let mut noise = scan_argument_noise(cx)?;
        let expression = match noise.next.clone() {
            Some(next) => parse_expression_node(cx, &next, &mut noise.nodes)?,
            None => None,
        };
        match expression {
            Some(_) => Ok(Some(region_with_last_as_content(noise.nodes))),
            None => {
                let at = noise.next.as_ref().map(|token| token.span).unwrap_or_else(|| {
                    Span::empty(cx.tokens.pos())
                });
                cx.recover(
                    ExpectedExpressionArgument::new(argument_name(spec)),
                    SourceSpan::new(&cx.source, at),
                )?;
                noise.rewind(cx);
                Ok(None)
            }
        }
    }

    /// The expression is mandatory: absent is a diagnosed recovery, not a valid match.
    fn can_match_empty(&self) -> bool {
        false
    }
}

/// A region whose **last** node is the content — the shape of every committed
/// expression (the noise, including the expression's own pre-space, precedes it).
fn region_with_last_as_content(nodes: Vec<BuildId>) -> ParsedArgumentNodes {
    debug_assert!(!nodes.is_empty(), "a committed expression staged its node");
    let last = nodes.len() as u32 - 1;
    ParsedArgumentNodes { nodes, content: ContentNodes::InRegion(last..last + 1) }
}

/// The standard mandatory-argument parser (pylatexenc's `'{'` shorthand as a core
/// parser): a group of the configured class if one opens at the position — its
/// delimiters are argument *syntax*, so the content is the group's children — with the
/// single-expression fallback otherwise (`\frac12`, `\frac1\alpha`; the expression
/// node is the content).
///
/// The delimiters come from the state's own group rules (the language declares `{…}`);
/// the parser is configured only with the group **class** that counts as this
/// argument's delimited form. Missing entirely (end of input, a paragraph break, an
/// enclosing group close): diagnosed here (tolerant) or abort (strict), argument
/// absent, nothing consumed.
pub struct GroupArgumentParser<L: Lang> {
    group_type: L::GroupTypeId,
}

impl<L: Lang> GroupArgumentParser<L> {
    /// A mandatory argument delimited by any group rule of class `group_type`, with
    /// the single-expression fallback.
    pub fn new(group_type: L::GroupTypeId) -> GroupArgumentParser<L> {
        GroupArgumentParser { group_type }
    }
}

impl<L: Lang> ArgumentParser<L> for GroupArgumentParser<L> {
    fn parse_argument(
        &self,
        cx: &mut ParseContext<'_, '_, L>,
        spec: &ArgumentSpec<L>,
    ) -> ConstructParserResult<L, Option<ParsedArgumentNodes>> {
        let mut noise = scan_argument_noise(cx)?;
        let Some(next) = noise.next.clone() else {
            return missing_mandatory(cx, noise, spec);
        };

        // The delimited form: a group open of the configured class.
        if let TokenKind::GroupOpen { rule, .. } = &next.kind {
            if rule.group_type == self.group_type {
                stage_pre_space(cx, &mut noise.nodes, next.pre_space)?;
                let rule = Arc::clone(rule);
                cx.tokens.move_past(&next, true);
                let mut group = GroupParser::new(next.span, rule);
                let (id, _delta) = group.parse(cx)?;
                let child_count = staged_child_count(cx, id);
                noise.nodes.push(id);
                return Ok(Some(ParsedArgumentNodes {
                    nodes: noise.nodes,
                    content: ContentNodes::InChildrenOf(id, 0..child_count),
                }));
            }
        }

        // The single-expression fallback.
        match parse_expression_node(cx, &next, &mut noise.nodes)? {
            Some(_) => Ok(Some(region_with_last_as_content(noise.nodes))),
            None => missing_mandatory(cx, noise, spec),
        }
    }

    /// The argument is mandatory: absent is a diagnosed recovery, not a valid match.
    fn can_match_empty(&self) -> bool {
        false
    }
}

/// The missing-mandatory recovery (§3.8): diagnostic at the blocking position
/// (tolerant) or abort (strict); absent, nothing consumed.
fn missing_mandatory<L: Lang>(
    cx: &mut ParseContext<'_, '_, L>,
    noise: ArgumentNoise<'_, L>,
    spec: &ArgumentSpec<L>,
) -> ConstructParserResult<L, Option<ParsedArgumentNodes>> {
    let at = noise
        .next
        .as_ref()
        .map(|token| token.span)
        .unwrap_or_else(|| Span::empty(cx.tokens.pos()));
    cx.recover(
        MissingMandatoryArgument::new(argument_name(spec)),
        SourceSpan::new(&cx.source, at),
    )?;
    noise.rewind(cx);
    Ok(None)
}

/// The number of children of a staged node (builder read-back).
fn staged_child_count<L: Lang>(cx: &ParseContext<'_, '_, L>, id: BuildId) -> u32 {
    let staged = cx.session.builder.staged_nodes();
    let view = staged.get(id).expect("the node was just staged");
    view.children().len() as u32
}

/// The standard optional-group argument parser (pylatexenc's `'['` shorthand as a core
/// parser): the argument is provided exactly when its opening delimiter comes next
/// (noise skipped), and absent otherwise — silently, consuming nothing.
///
/// The delimiters are **minted for the occasion**: the parser carries its own
/// [`GroupRule`] (say `[`…`]` under a preset's option class), prepended to the current
/// group rules (prepended so it wins ties against a same-spelling rule already in
/// scope) in a derived state that scopes the argument's **whole extent** — the probing
/// peek and the group's contents alike, so nested brackets **balance**:
/// `[with[recursive[use]of]brackets]` is one argument with nested group nodes
/// (user-decided July 2026, pylatexenc parity; supersedes the briefly-shipped
/// LaTeX-style first-`]`-closes rule).
///
/// Child descents from the contents are policy-steered ([`ChildStateSpec`] through
/// [`GroupParser::with_child_states`] — the direct translation of pylatexenc's
/// `LatexDelimitedGroupParserInfo.make_child_parsing_state`): a nested group opened by
/// the minted rule keeps the contents state (that is what balances recursively), while
/// **any other child** — a brace group, an invocation — reverts to the argument's own
/// state, where `]` is an ordinary character: braces protect, `[{arg with ]}]` holds.
/// (As in pylatexenc, the protection policy rides one bracket level: pathological
/// mixtures like `[a[{x]y}b]` mangle there silently and mangle here *with*
/// diagnostics.)
///
/// **Protection presupposes the close spelling is not otherwise special in the
/// argument state.** If the base rules class `[`/`]` as a genuine group pairing of the
/// language, the reverted state reads `]` as a real close token, and `\item[{a]b}]`
/// genuinely fails — stray-close unwinding with diagnostics, exactly like `{a]b}`
/// anywhere else in that language. Intended, not degradation: the revert restores the
/// language's own reading, it never overrides it (user-decided July 2026,
/// DESIGN_RATIONALE.md §3.6).
///
/// Content designation: the option group's children — except that a **lone child group
/// of the configured protective class** (`[{arg with ]}]`: braces protecting the `]`)
/// designates *that* group's children instead, the parse-time resolution of
/// pylatexenc's post-hoc `unwrap_double_group` accessor hack (§3.5).
///
/// [`ChildStateSpec`]: super::ChildStateSpec
pub struct OptionalGroupArgumentParser<L: Lang> {
    rule: Arc<GroupRule<L>>,
    unwrap_lone_group: Option<L::GroupTypeId>,
}

impl<L: Lang> OptionalGroupArgumentParser<L> {
    /// An optional argument delimited by `rule` (e.g. `[`…`]` under a preset's option
    /// class), with no protective-group unwrapping.
    pub fn new(rule: Arc<GroupRule<L>>) -> OptionalGroupArgumentParser<L> {
        OptionalGroupArgumentParser { rule, unwrap_lone_group: None }
    }

    /// Designate the children of a lone child group of class `group_type` as the
    /// content (the protective-braces idiom — see the type docs).
    pub fn with_unwrap_lone_group(mut self, group_type: L::GroupTypeId) -> Self {
        self.unwrap_lone_group = Some(group_type);
        self
    }
}

impl<L: Lang> ArgumentParser<L> for OptionalGroupArgumentParser<L> {
    fn parse_argument(
        &self,
        cx: &mut ParseContext<'_, '_, L>,
        _spec: &ArgumentSpec<L>,
    ) -> ConstructParserResult<L, Option<ParsedArgumentNodes>> {
        let mut noise = scan_argument_noise(cx)?;
        if noise.next.is_none() {
            noise.rewind(cx);
            return Ok(None);
        };

        // The state with the minted rule in force — used for the probing peek and,
        // when the argument is present, for the group's contents (type docs). `Arc`
        // identity of the matched rule is the match criterion — the delta supplied
        // exactly this `Arc`.
        let mut groups = alloc::vec![Arc::clone(&self.rule)];
        groups.extend(cx.state.rules().groups.iter().cloned());
        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: Some(groups),
            ..TokenRulesOverrides::default()
        });
        let contents_state = cx.session.derived_state(&cx.state, &delta);
        let argument_state = Arc::clone(&cx.state);
        let matched = match cx.probe_token(&contents_state)? {
            Some(token)
                if matches!(
                    &token.kind,
                    TokenKind::GroupOpen { rule, .. } if Arc::ptr_eq(rule, &self.rule)
                ) =>
            {
                Some(token)
            }
            _ => None,
        };
        let Some(open) = matched else {
            noise.rewind(cx);
            return Ok(None);
        };

        stage_pre_space(cx, &mut noise.nodes, open.pre_space)?;
        cx.tokens.move_past(&open, true);
        // pylatexenc's `make_child_parsing_state`, expressed as the decided §3.6
        // descent policy: a nested group opened by the minted rule keeps the contents
        // state (nested brackets balance recursively — the rule then rides the
        // inherited states of deeper levels), while any other child descent — a brace
        // group, an invocation — reverts to the argument's own state, where the close
        // delimiter is an ordinary character (`[{arg with ]}]`).
        //
        // The callback's first parameter is the state at the *descent site* — the group
        // interior state, not the contents state. Returning the captured contents state
        // instead is data-equivalent (the interior derivation overrides
        // expecting_group_close either way) and keys every nested same-rule descent on
        // (contents state, rule) — the same derivation as the outer descent, an
        // immediate memo hit.
        let contents_for_children = Arc::clone(&contents_state);
        let argument_for_children = Arc::clone(&argument_state);
        let keep_or_revert = move |_descent: &Arc<ParsingState<L>>, token: &Token<'_, L>| {
            if let TokenKind::GroupOpen { rule, .. } = &token.kind {
                if Arc::ptr_eq(rule, &self.rule) {
                    return Arc::clone(&contents_for_children);
                }
            }
            Arc::clone(&argument_for_children)
        };
        let child_states = ChildStateSpec {
            group: GroupChildState::Compute(&keep_or_revert),
            invocation: InvocationChildState::Fixed(Arc::clone(&argument_state)),
        };
        let mut group =
            GroupParser::new(open.span, Arc::clone(&self.rule)).with_child_states(child_states);
        let (id, _delta) = cx.parse_scoped(contents_state, &mut group)?;

        // Content: the option group's children, or a lone protective child group's.
        let (content_parent, content_len) = {
            let staged = cx.session.builder.staged_nodes();
            let children = staged.get(id).expect("the group was just staged").children();
            let unwrapped = match (self.unwrap_lone_group, children) {
                (Some(protective), [only]) => {
                    let inner = staged.get(*only).expect("staged child");
                    matches!(
                        inner.kind(),
                        NodeKind::Group(data) if data.group_type == Some(protective)
                    )
                    .then_some(*only)
                }
                _ => None,
            };
            let parent = unwrapped.unwrap_or(id);
            let len = staged.get(parent).expect("staged node").children().len() as u32;
            (parent, len)
        };
        noise.nodes.push(id);
        Ok(Some(ParsedArgumentNodes {
            nodes: noise.nodes,
            content: ContentNodes::InChildrenOf(content_parent, 0..content_len),
        }))
    }

    /// Optional: absent is a valid, silent outcome (the trait default, stated
    /// explicitly — the expression-position guard leans on this answer).
    fn can_match_empty(&self) -> bool {
        true
    }
}

/// The literal-marker argument parser (pylatexenc's `LatexOptionalCharsMarkerParser`,
/// the `'*'` shorthand as a core parser): the argument is provided exactly when the
/// marker's characters come next (noise skipped; the chars must be consecutive, with
/// no intervening whitespace), staged as a single `Chars` node which **is** the
/// content (pylatexenc parity). Absent is silent, consuming nothing.
pub struct MarkerArgumentParser {
    marker: Box<str>,
}

impl MarkerArgumentParser {
    /// An optional literal marker (e.g. `*`). Must be non-empty.
    pub fn new(marker: impl Into<Box<str>>) -> MarkerArgumentParser {
        let marker = marker.into();
        debug_assert!(!marker.is_empty(), "a marker needs at least one character");
        MarkerArgumentParser { marker }
    }
}

impl<L: Lang> ArgumentParser<L> for MarkerArgumentParser {
    fn parse_argument(
        &self,
        cx: &mut ParseContext<'_, '_, L>,
        _spec: &ArgumentSpec<L>,
    ) -> ConstructParserResult<L, Option<ParsedArgumentNodes>> {
        let mut noise = scan_argument_noise(cx)?;
        let (Some(first), Some(first_char)) = (noise.next.clone(), self.marker.chars().next())
        else {
            noise.rewind(cx);
            return Ok(None);
        };
        if !matches!(first.kind, TokenKind::Char(c) if c == first_char) {
            noise.rewind(cx);
            return Ok(None);
        }
        let mut span = first.span;
        cx.tokens.move_past(&first, true);
        let state = Arc::clone(&cx.state);
        for expected in self.marker.chars().skip(1) {
            let Some(token) = cx.probe_token(&state)? else {
                noise.rewind(cx);
                return Ok(None);
            };
            let continues_marker = matches!(token.kind, TokenKind::Char(c) if c == expected)
                && token.pre_space.is_empty()
                && token.span.start() == span.end();
            if !continues_marker {
                noise.rewind(cx);
                return Ok(None);
            }
            span.extend_to(token.span.end());
            cx.tokens.move_past(&token, true);
        }
        stage_pre_space(cx, &mut noise.nodes, first.pre_space)?;
        noise.nodes.push(stage(cx, NodeKind::chars(span), span)?);
        Ok(Some(region_with_last_as_content(mem::take(&mut noise.nodes))))
    }

    /// Optional: absent is a valid, silent outcome (the trait default, stated
    /// explicitly — the expression-position guard leans on this answer).
    fn can_match_empty(&self) -> bool {
        true
    }
}

// Manual Debug impls where derives would demand `L:` bounds.

impl<L: Lang> fmt::Debug for GroupArgumentParser<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroupArgumentParser")
            .field("group_type", &self.group_type)
            .finish()
    }
}

impl<L: Lang> fmt::Debug for OptionalGroupArgumentParser<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OptionalGroupArgumentParser")
            .field("rule", &self.rule)
            .field("unwrap_lone_group", &self.unwrap_lone_group)
            .finish()
    }
}

impl fmt::Debug for MarkerArgumentParser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MarkerArgumentParser").field("marker", &self.marker).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        ChildStateSpec, ConstructParser, NodesParser, StopCause, StopSpec, UnclosedGroup,
        UnclosedGroupFound,
    };
    use super::*;
    use crate::engine::{ParseResult, ParserSession};
    use crate::error::{ParseError, Recovery};
    use crate::library::{CallableQuery, CallableSyntax, Library, LibraryStack};
    use crate::node::NodeRef;
    use crate::source::Source;
    use crate::spec::{CallableSpec, StdCallableSpec};
    use crate::state::{ParsingState, ResolvedCallable, StateData};
    use crate::token::{
        CommandRule, CommentRule, StdTokenReader, TokenListReader, TokenReader, TokenRules,
        WhitespaceRules,
    };
    use alloc::string::ToString;
    use alloc::vec;

    const GT_BRACE: u32 = 0;
    const GT_OPTION: u32 = 2;
    const CT_MACRO: u32 = 10;

    /// Test lang resolving `Command` tokens against the state's libraries under the
    /// `CT_MACRO` form (the preset resolution pattern of the 6.4 tests).
    #[derive(Debug, Clone, Copy)]
    struct ArgLang;
    impl Lang for ArgLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();

        fn resolve_command(
            state: &ParsingState<Self>,
            token: &Token<'_, Self>,
        ) -> Option<ResolvedCallable<Self>> {
            let TokenKind::Command { name, escape_char, .. } = &token.kind else {
                return None;
            };
            let query = CallableQuery::new(
                CT_MACRO,
                name,
                CallableSyntax::Command { escape_char: *escape_char },
            )
            .with_token(token);
            let spec = state.libraries().resolve(&query, state)?;
            Some(ResolvedCallable { callable_type: CT_MACRO, spec })
        }
    }

    fn rules() -> TokenRules<ArgLang> {
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

    // --- spec builders ----------------------------------------------------------------

    fn brace_arg() -> Arc<ArgumentSpec<ArgLang>> {
        Arc::new(ArgumentSpec::new(Arc::new(GroupArgumentParser::new(GT_BRACE))))
    }

    fn option_rule() -> Arc<GroupRule<ArgLang>> {
        Arc::new(GroupRule { group_type: GT_OPTION, open: "[".into(), close: "]".into() })
    }

    fn optional_arg() -> Arc<ArgumentSpec<ArgLang>> {
        Arc::new(ArgumentSpec::new(Arc::new(OptionalGroupArgumentParser::new(option_rule()))))
    }

    fn optional_arg_unwrapping() -> Arc<ArgumentSpec<ArgLang>> {
        Arc::new(ArgumentSpec::new(Arc::new(
            OptionalGroupArgumentParser::new(option_rule()).with_unwrap_lone_group(GT_BRACE),
        )))
    }

    fn marker_arg(marker: &str) -> Arc<ArgumentSpec<ArgLang>> {
        Arc::new(ArgumentSpec::new(Arc::new(MarkerArgumentParser::new(marker))))
    }

    fn expression_arg() -> Arc<ArgumentSpec<ArgLang>> {
        Arc::new(ArgumentSpec::new(Arc::new(ExpressionParser::new())))
    }

    /// A state whose library defines each named macro with the given argument specs.
    fn state_with(
        macros: &[(&str, Vec<Arc<ArgumentSpec<ArgLang>>>)],
    ) -> Arc<ParsingState<ArgLang>> {
        state_with_specs(
            &macros
                .iter()
                .map(|(name, arguments)| {
                    let spec: Arc<dyn CallableSpec<ArgLang>> =
                        Arc::new(StdCallableSpec::new(arguments.clone()));
                    (*name, spec)
                })
                .collect::<Vec<_>>(),
        )
    }

    fn state_with_specs(
        macros: &[(&str, Arc<dyn CallableSpec<ArgLang>>)],
    ) -> Arc<ParsingState<ArgLang>> {
        let mut lib = Library::new("test-macros");
        for (name, spec) in macros {
            lib.insert(CT_MACRO, *name, Arc::clone(spec));
        }
        let mut libraries = LibraryStack::new();
        libraries.push(Arc::new(lib));
        Arc::new(ParsingState::new(StateData { rules: rules(), libraries, ext: () }))
    }

    // --- harness (the 6.2 driving pattern, compact) -------------------------------------

    struct Parsed {
        result: ParseResult<ArgLang>,
        pos: usize,
    }

    impl fmt::Debug for Parsed {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Parsed")
                .field("result", &self.result)
                .field("pos", &self.pos)
                .finish()
        }
    }

    /// Drive a `NodesParser` to end of input over `tokens`, stage the outcome under a
    /// root `List` spanning the parsed extent, freeze, and run the invariant checker.
    fn try_run<'s>(
        content: &'s str,
        tokens: &mut dyn TokenReader<'s, ArgLang>,
        state: &Arc<ParsingState<ArgLang>>,
        recovery: Recovery,
    ) -> Result<Parsed, ParseError> {
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut session = ParserSession::new(recovery);
        let mut cx = ParseContext::new(tokens, Arc::clone(&source), Arc::clone(state), &mut session);
        let mut parser = NodesParser::new(StopSpec::none())
            .with_child_states(ChildStateSpec::inherit());
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

    /// [`try_run`] over a `StdTokenReader` — the driver for tests whose tokenization is
    /// state-dependent (optional-argument probing, per-argument rule deltas): a
    /// pre-scanned token list cannot re-tokenize under momentary states
    /// (`TokenListReader`'s documented fidelity limit).
    fn parse_std(
        content: &str,
        state: &Arc<ParsingState<ArgLang>>,
        recovery: Recovery,
    ) -> Parsed {
        let mut reader = StdTokenReader::new(content);
        try_run(content, &mut reader, state, recovery).expect("parse")
    }

    /// Run against both readers (report R6) and assert they agree; for content whose
    /// tokenization is state-independent.
    fn parse_both(
        content: &str,
        state: &Arc<ParsingState<ArgLang>>,
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

    fn root_child(parsed: &Parsed, i: usize) -> NodeRef<'_, ArgLang> {
        parsed.result.tree.root().child(i).expect("root child")
    }

    /// The resolved content nodes of argument `i`, asserted present.
    fn content_of(callable: NodeRef<'_, ArgLang>, i: usize) -> Vec<NodeRef<'_, ArgLang>> {
        callable.argument_content_nodes(i).expect("provided argument").collect()
    }

    // --- the delimited form and the expression fallback ---------------------------------

    #[test]
    fn brace_group_arguments_designate_group_children_as_content() {
        let st = state_with(&[("frac", vec![brace_arg(), brace_arg()])]);
        let parsed = parse_both(r"\frac{a}{b} x", &st, Recovery::Strict);

        let frac = root_child(&parsed, 0);
        assert!(frac.is_callable());
        assert_eq!(frac.name(), Some("frac"));
        assert_eq!(frac.span().range(), 0..11);
        // The trigger's own post-space: empty, `{` follows the name directly.
        assert_eq!(frac.post_space(), Some(""));
        // Children = the two argument regions (raw-syntax view).
        assert_eq!(frac.child_count(), 2);
        assert_eq!(frac.child(0).unwrap().group_delimiters(), Some(("{", "}")));

        // Content: the groups' children, braces excluded — parser-designated.
        let content0 = content_of(frac, 0);
        assert_eq!(content0.len(), 1);
        assert_eq!(content0[0].chars(), Some("a"));
        assert_eq!(content_of(frac, 1)[0].chars(), Some("b"));

        // The whitespace after the last argument is sibling content, not post-space.
        assert_eq!(root_child(&parsed, 1).chars(), Some(" x"));
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn single_token_fallback_stages_whitespace_as_region_noise() {
        let st = state_with(&[("frac", vec![brace_arg(), brace_arg()])]);
        let parsed = parse_both(r"\frac 1 2", &st, Recovery::Strict);

        let frac = root_child(&parsed, 0);
        assert_eq!(frac.span().range(), 0..9);
        // The name-terminating whitespace is the recorded post-space — between the
        // name and the first region (§3.5 invariant 3 as amended).
        assert_eq!(frac.post_space(), Some(" "));
        // Children: "1", the inter-argument whitespace node, "2".
        assert_eq!(frac.child_count(), 3);
        assert_eq!(frac.child(0).unwrap().chars(), Some("1"));
        assert_eq!(frac.child(1).unwrap().chars(), Some(" "));
        assert_eq!(frac.child(2).unwrap().chars(), Some("2"));

        // Regions: arg 0 = ["1"]; arg 1 = [" ", "2"] with only "2" as content.
        let region1: Vec<_> = frac.argument_nodes(1).unwrap().collect();
        assert_eq!(region1.len(), 2);
        let content1 = content_of(frac, 1);
        assert_eq!(content1.len(), 1);
        assert_eq!(content1[0].chars(), Some("2"));
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn expression_fallback_dispatches_a_full_invocation() {
        let st =
            state_with(&[("frac", vec![brace_arg(), brace_arg()]), ("alpha", vec![])]);
        let parsed = parse_both(r"\frac1\alpha", &st, Recovery::Strict);

        let frac = root_child(&parsed, 0);
        assert_eq!(frac.span().range(), 0..12);
        let content1 = content_of(frac, 1);
        assert_eq!(content1.len(), 1);
        assert!(content1[0].is_callable());
        assert_eq!(content1[0].name(), Some("alpha"));
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn comments_between_arguments_are_region_noise() {
        let st = state_with(&[("frac", vec![brace_arg(), brace_arg()])]);
        let parsed = parse_both("\\frac %half\n{1}{2}", &st, Recovery::Strict);

        let frac = root_child(&parsed, 0);
        assert_eq!(frac.span().range(), 0..18);
        // Arg 0's region: the comment node, then the group; content = the group's
        // children only (noise kept, out of content's way).
        let region0: Vec<_> = frac.argument_nodes(0).unwrap().collect();
        assert_eq!(region0.len(), 2);
        assert!(region0[0].is_comment());
        assert_eq!(region0[0].comment(), Some("half"));
        assert!(region0[1].is_group());
        let content0 = content_of(frac, 0);
        assert_eq!(content0.len(), 1);
        assert_eq!(content0[0].chars(), Some("1"));
        assert!(parsed.result.diagnostics.is_empty());
    }

    // --- markers -------------------------------------------------------------------------

    #[test]
    fn star_marker_present_is_a_chars_content_node() {
        let st = state_with(&[(
            "section",
            vec![
                Arc::new(
                    ArgumentSpec::new(Arc::new(MarkerArgumentParser::new("*"))).named("star"),
                ),
                brace_arg(),
            ],
        )]);
        let parsed = parse_both(r"\section*{T}", &st, Recovery::Strict);

        let section = root_child(&parsed, 0);
        assert_eq!(section.span().range(), 0..12);
        assert_eq!(section.child_count(), 2);
        let star = content_of(section, 0);
        assert_eq!(star.len(), 1);
        assert_eq!(star[0].chars(), Some("*"));
        // By-name access through the self-describing record.
        let args = section.arguments().unwrap();
        assert!(args.get_named("star").unwrap().is_provided());
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn absent_marker_is_silent() {
        let st = state_with(&[("section", vec![marker_arg("*"), brace_arg()])]);
        let parsed = parse_both(r"\section{T}", &st, Recovery::Strict);

        let section = root_child(&parsed, 0);
        let args = section.arguments().unwrap();
        assert!(!args.get(0).unwrap().is_provided());
        assert!(args.get(1).unwrap().is_provided());
        assert_eq!(section.child_count(), 1); // only the title group
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn marker_after_whitespace_stages_the_noise_node() {
        // The marker's pre-space becomes region noise (`\x{a} *`: the space belongs to
        // the marker's region, the marker node is the content).
        let st = state_with(&[("x", vec![brace_arg(), marker_arg("*")])]);
        let parsed = parse_both(r"\x{a} *", &st, Recovery::Strict);

        let x = root_child(&parsed, 0);
        assert_eq!(x.span().range(), 0..7);
        let region1: Vec<_> = x.argument_nodes(1).unwrap().collect();
        assert_eq!(region1.len(), 2);
        assert_eq!(region1[0].chars(), Some(" "));
        let content1 = content_of(x, 1);
        assert_eq!(content1.len(), 1);
        assert_eq!(content1[0].chars(), Some("*"));
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn multi_char_marker_matches_whole_or_not_at_all() {
        let st = state_with(&[("m", vec![marker_arg("**")])]);

        let parsed = parse_both(r"\m**z", &st, Recovery::Strict);
        let m = root_child(&parsed, 0);
        assert_eq!(content_of(m, 0)[0].chars(), Some("**"));
        assert_eq!(root_child(&parsed, 1).chars(), Some("z"));

        // A partial match consumes nothing: `*z` stays sibling content.
        let parsed = parse_both(r"\m*z", &st, Recovery::Strict);
        let m = root_child(&parsed, 0);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(m.child_count(), 0);
        assert_eq!(root_child(&parsed, 1).chars(), Some("*z"));
        assert!(parsed.result.diagnostics.is_empty());
    }

    // --- optional groups (StdTokenReader only: the probe re-tokenizes) ------------------

    #[test]
    fn optional_group_present() {
        let st = state_with(&[("item", vec![optional_arg()])]);
        let parsed = parse_std(r"\item[label] x", &st, Recovery::Strict);

        let item = root_child(&parsed, 0);
        assert_eq!(item.span().range(), 0..12);
        assert_eq!(item.child_count(), 1);
        let option = item.child(0).unwrap();
        assert_eq!(option.group_type(), Some(GT_OPTION));
        assert_eq!(option.group_delimiters(), Some(("[", "]")));
        let content = content_of(item, 0);
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].chars(), Some("label"));
        assert_eq!(root_child(&parsed, 1).chars(), Some(" x"));
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn optional_group_absent_is_silent_and_consumes_nothing() {
        let st = state_with(&[("item", vec![optional_arg()])]);
        let parsed = parse_std(r"\item x", &st, Recovery::Strict);

        let item = root_child(&parsed, 0);
        // The trigger's own post-space; the argument-less span is the trigger's.
        assert_eq!(item.span().range(), 0..6);
        assert_eq!(item.post_space(), Some(" "));
        assert!(!item.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(root_child(&parsed, 1).chars(), Some("x"));
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn absent_optional_probe_rewinds_and_noise_reparses_as_enclosing_content() {
        // The probe/rewind case from §3.5: the scan consumes the comment while looking
        // for `[`, finds `x`, rewinds — the same comment re-parses as sibling content,
        // and the speculatively staged nodes are dropped by the builder.
        let st = state_with(&[("item", vec![optional_arg()])]);
        let parsed = parse_std("\\item % c\nx", &st, Recovery::Strict);

        let item = root_child(&parsed, 0);
        assert_eq!(item.span().range(), 0..6);
        assert_eq!(item.child_count(), 0);
        let comment = root_child(&parsed, 1);
        assert!(comment.is_comment());
        assert_eq!(comment.comment(), Some(" c"));
        assert_eq!(root_child(&parsed, 2).chars(), Some("x"));
        // Exactly root + callable + comment + chars: the abandoned probe nodes are gone.
        assert_eq!(parsed.result.tree.node_count(), 4);
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn optional_lone_protective_group_designates_inner_content() {
        // `[{arg with ]}]`: the braces protect the `]`; the parser designates the
        // *inner* group's children as content at parse time (§3.5 — the parse-time
        // resolution of pylatexenc's unwrap_double_group accessor hack).
        let st = state_with(&[("item", vec![optional_arg_unwrapping()])]);
        let parsed = parse_std(r"\item[{arg with ]}]", &st, Recovery::Strict);

        let item = root_child(&parsed, 0);
        assert_eq!(item.span().range(), 0..19);
        let content = content_of(item, 0);
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].chars(), Some("arg with ]"));
        // The content parent is the inner brace group.
        let region = item.arguments().unwrap().get(0).unwrap().region.as_ref().unwrap();
        let parent = parsed.result.tree.node(region.content_parent());
        assert_eq!(parent.group_type(), Some(GT_BRACE));
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn optional_lone_group_with_whitespace_is_not_unwrapped() {
        // `[ {a} ]` has three interior nodes (ws, group, ws): no lone protective group,
        // content = the option group's own children.
        let st = state_with(&[("item", vec![optional_arg_unwrapping()])]);
        let parsed = parse_std(r"\item[ {a} ]", &st, Recovery::Strict);

        let item = root_child(&parsed, 0);
        let content = content_of(item, 0);
        assert_eq!(content.len(), 3);
        assert!(content[1].is_group());
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn optional_unwrap_is_opt_in() {
        let st = state_with(&[("item", vec![optional_arg()])]);
        let parsed = parse_std(r"\item[{a}]", &st, Recovery::Strict);

        let content = content_of(root_child(&parsed, 0), 0);
        assert_eq!(content.len(), 1);
        assert!(content[0].is_group());
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn optional_brackets_balance_into_nested_groups() {
        // User-decided (July 2026, 6.5 review): pylatexenc parity — the minted rule is
        // in force for the group's contents, so nested brackets balance into ONE
        // argument with nested group nodes (shape verified against pylatexenc 3.0a33
        // on this exact input; supersedes the briefly-shipped first-`]`-closes rule).
        let st = state_with(&[("item", vec![optional_arg()])]);
        let parsed =
            parse_std(r"\item[with[recursive[use]of]brackets] x", &st, Recovery::Strict);

        let item = root_child(&parsed, 0);
        assert_eq!(item.span().range(), 0..37);
        assert_eq!(item.child_count(), 1);
        let option = item.child(0).unwrap();
        assert_eq!(option.span().range(), 5..37);
        assert_eq!(option.group_type(), Some(GT_OPTION));

        // Content = the option group's children: chars, nested group, chars.
        let content = content_of(item, 0);
        assert_eq!(content.len(), 3);
        assert_eq!(content[0].chars(), Some("with"));
        assert_eq!(content[2].chars(), Some("brackets"));
        let nested = content[1];
        assert!(nested.is_group());
        assert_eq!(nested.span().range(), 10..28);
        assert_eq!(nested.group_type(), Some(GT_OPTION));
        assert_eq!(nested.child_count(), 3);
        assert_eq!(nested.child(0).unwrap().chars(), Some("recursive"));
        assert_eq!(nested.child(2).unwrap().chars(), Some("of"));
        let deep = nested.child(1).unwrap();
        assert_eq!(deep.span().range(), 20..25);
        assert_eq!(deep.child(0).unwrap().chars(), Some("use"));

        assert_eq!(root_child(&parsed, 1).chars(), Some(" x"));
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn optional_child_invocations_revert_to_the_argument_state() {
        // pylatexenc's make_child_parsing_state semantics: a child that is not a
        // minted-rule group parses under the argument's own state — inside `\m`'s
        // brace argument, `]` is an ordinary character and cannot close the option.
        let st = state_with(&[("item", vec![optional_arg()]), ("m", vec![brace_arg()])]);
        let parsed = parse_std(r"\item[\m{a]b}] x", &st, Recovery::Strict);

        let item = root_child(&parsed, 0);
        assert_eq!(item.span().range(), 0..14);
        let content = content_of(item, 0);
        assert_eq!(content.len(), 1);
        let m = content[0];
        assert_eq!(m.name(), Some("m"));
        assert_eq!(content_of(m, 0)[0].chars(), Some("a]b"));
        assert_eq!(root_child(&parsed, 1).chars(), Some(" x"));
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn brackets_as_language_groups_defeat_brace_protection_by_design() {
        // User-decided (July 2026, Action-06 review; DESIGN_RATIONALE §3.6): brace
        // protection works because the *reverted* argument state reads `]` as an
        // ordinary character. When the language's own base rules class `[`/`]` as a
        // genuine group pairing, the reverted state reads `]` as a real group-close
        // token — `\item[{a]b}]` must then genuinely fail (stray-close unwinding, with
        // diagnostics), exactly like `{a]b}` anywhere else in that language.
        const GT_BRACKET: u32 = 3;
        let mut bracket_rules = rules();
        bracket_rules.groups.push(Arc::new(GroupRule {
            group_type: GT_BRACKET,
            open: "[".into(),
            close: "]".into(),
        }));
        let mut lib = Library::new("test-macros");
        lib.insert(
            CT_MACRO,
            "item",
            Arc::new(StdCallableSpec::new(vec![optional_arg_unwrapping()]))
                as Arc<dyn CallableSpec<ArgLang>>,
        );
        let mut libraries = LibraryStack::new();
        libraries.push(Arc::new(lib));
        let st = Arc::new(ParsingState::new(StateData {
            rules: bracket_rules,
            libraries,
            ext: (),
        }));

        // The plain case is unaffected: the minted option rule is prepended and wins
        // the same-spelling tie in the contents state.
        let clean = parse_std(r"\item[a] x", &st, Recovery::Strict);
        assert!(clean.result.diagnostics.is_empty());
        assert_eq!(content_of(root_child(&clean, 0), 0)[0].chars(), Some("a"));

        // The would-be-protected case fails for real.
        let content = r"\item[{a]b}]";
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(content);
        let mut session = ParserSession::new(Recovery::Tolerant);
        let mut cx = ParseContext::new(
            &mut reader,
            Arc::clone(&source),
            Arc::clone(&st),
            &mut session,
        );
        let mut parser =
            NodesParser::new(StopSpec::none()).with_child_states(ChildStateSpec::inherit());
        let (outcome, _delta) = parser.parse(&mut cx).expect("tolerant parse");

        // The brace group closed early at the stray `]` (which the option level then
        // claimed as its close), so the later `}` has no owner and escapes to this
        // root loop — the input does not silently hold together.
        assert!(matches!(
            outcome.stop,
            StopCause::UnexpectedGroupClose { .. }
        ));
        drop(cx);
        assert!(session.diagnostics.has_errors());
        assert!(session.diagnostics.conditions::<UnclosedGroup>().any(|c| {
            c.expected_close == "}" && c.found == UnclosedGroupFound::StrayClose
        }));
    }

    // --- missing-mandatory recovery ------------------------------------------------------

    #[test]
    fn missing_mandatory_is_diagnosed_and_absent() {
        let st = state_with(&[(
            "frac",
            vec![
                brace_arg(),
                Arc::new(
                    ArgumentSpec::new(Arc::new(GroupArgumentParser::new(GT_BRACE)))
                        .named("denominator"),
                ),
            ],
        )]);
        let parsed = parse_std(r"\frac{a}", &st, Recovery::Tolerant);

        let frac = root_child(&parsed, 0);
        assert_eq!(frac.span().range(), 0..8);
        let args = frac.arguments().unwrap();
        assert!(args.get(0).unwrap().is_provided());
        assert!(!args.get(1).unwrap().is_provided());
        assert_eq!(parsed.result.diagnostics.len(), 1);
        let message = parsed.result.diagnostics.iter().next().unwrap().message().to_string();
        assert!(
            message.contains("missing mandatory argument") && message.contains("denominator"),
            "unexpected message: {message}"
        );

        // Strict mode aborts instead.
        let mut reader = StdTokenReader::new(r"\frac{a}");
        let err = try_run(r"\frac{a}", &mut reader, &st, Recovery::Strict).unwrap_err();
        assert!(err.to_string().contains("missing mandatory argument"));
    }

    #[test]
    fn missing_mandatory_at_an_enclosing_group_close() {
        // `{\frac}`: the close is the enclosing group's business — both arguments are
        // reported missing without consuming it, and the group closes normally.
        let st = state_with(&[("frac", vec![brace_arg(), brace_arg()])]);
        let parsed = parse_std(r"{\frac}", &st, Recovery::Tolerant);

        let group = root_child(&parsed, 0);
        assert!(group.is_group());
        assert_eq!(group.span().range(), 0..7);
        assert_eq!(group.group_delimiters(), Some(("{", "}")));
        let frac = group.child(0).unwrap();
        assert!(frac.is_callable());
        assert_eq!(frac.span().range(), 1..6);
        assert!(frac.arguments().unwrap().iter().all(|arg| !arg.is_provided()));
        assert_eq!(parsed.result.diagnostics.len(), 2);
    }

    #[test]
    fn unrecoverable_token_error_while_probing_aborts_even_tolerant() {
        // The probe_token recoverability rule (§3.8): a token error carrying no recovery
        // must abort even under Tolerant — reporting the argument absent instead would
        // record a spurious missing-argument diagnostic before the enclosing loop's
        // re-read aborted anyway.
        use crate::token::{EndOfStreamAfterEscape, TokenError, TokenErrorKind, TokenResult};

        struct BrokenReader;
        impl<'s> TokenReader<'s, ArgLang> for BrokenReader {
            fn peek(
                &mut self,
                _state: &Arc<ParsingState<ArgLang>>,
            ) -> TokenResult<'s, ArgLang, Token<'s, ArgLang>> {
                Err(TokenError::new(
                    TokenErrorKind::EndOfStreamAfterEscape(EndOfStreamAfterEscape::new(
                        '\\',
                    )),
                    Span::new(0, 1),
                    None, // unrecoverable
                ))
            }

            fn move_past(&mut self, _token: &Token<'s, ArgLang>, _skip_post_space: bool) {}

            fn move_to(&mut self, _token: &Token<'s, ArgLang>, _rewind_pre_space: bool) {}

            fn move_to_pos(&mut self, _pos: usize) {}

            fn pos(&self) -> usize {
                0
            }
        }

        let source: Arc<Source> = Arc::new(Source::new("x"));
        let st = state_with(&[]);
        let mut reader = BrokenReader;
        let mut session = ParserSession::new(Recovery::Tolerant);
        let mut cx = ParseContext::new(&mut reader, Arc::clone(&source), Arc::clone(&st), &mut session);

        let spec = brace_arg();
        let err = spec
            .parser
            .parse_argument(&mut cx, &spec)
            .expect_err("an unrecoverable token error must abort the probe");
        assert_eq!(err.identifier(), EndOfStreamAfterEscape::IDENTIFIER);
        // No spurious absent-argument diagnostic was recorded on the way out.
        assert!(session.diagnostics.is_empty());
    }

    #[test]
    fn argument_diagnostics_carry_argument_and_invocation_frames() {
        // `\frac{a}` (tolerant): the missing second argument is diagnosed under two
        // frames — the argument's (innermost) and the invocation's — titled through the
        // spec's defaulted stack_frame_title hook at snapshot time.
        let st = state_with(&[("frac", vec![brace_arg(), brace_arg()])]);
        let parsed = parse_std(r"\frac{a}", &st, Recovery::Tolerant);

        let diagnostic = parsed
            .result
            .diagnostics
            .with_identifier(MissingMandatoryArgument::IDENTIFIER)
            .next()
            .unwrap();
        let titles: Vec<&str> = diagnostic.frames().iter().map(|f| f.title()).collect();
        assert_eq!(titles, ["argument #2 of ‘\\frac’", "callable ‘\\frac’"]);
        // The argument frame is anchored where the argument's region starts; the
        // invocation frame at the trigger.
        assert_eq!(diagnostic.frames()[0].span().range(), 8..8);
        assert_eq!(diagnostic.frames()[1].span().range(), 0..5);

        // A strict abort carries the same snapshot on the ParseError.
        let mut reader = StdTokenReader::new(r"\frac{a}");
        let err = try_run(r"\frac{a}", &mut reader, &st, Recovery::Strict).unwrap_err();
        let titles: Vec<&str> = err.frames().iter().map(|f| f.title()).collect();
        assert_eq!(titles, ["argument #2 of ‘\\frac’", "callable ‘\\frac’"]);
    }

    // --- expression-position rules -------------------------------------------------------

    #[test]
    fn callable_requiring_arguments_cannot_be_an_expression() {
        // pylatexenc's requires-arguments diagnostic: `\b` (which takes an argument)
        // used as `\a`'s single-token argument stages bare — trigger alone, its
        // argument absent — leaving `{x}` as sibling content.
        let st = state_with(&[("a", vec![brace_arg()]), ("b", vec![brace_arg()])]);
        let parsed = parse_both(r"\a\b{x}", &st, Recovery::Tolerant);

        let a = root_child(&parsed, 0);
        assert_eq!(a.span().range(), 0..4);
        let content = content_of(a, 0);
        assert_eq!(content.len(), 1);
        let b = content[0];
        assert_eq!(b.name(), Some("b"));
        assert_eq!(b.child_count(), 0);
        assert!(!b.arguments().unwrap().get(0).unwrap().is_provided());
        // `{x}` was not swallowed by `\b`.
        assert!(root_child(&parsed, 1).is_group());
        assert_eq!(parsed.result.diagnostics.len(), 1);
        let message = parsed.result.diagnostics.iter().next().unwrap().message().to_string();
        assert!(message.contains("single expression"), "unexpected message: {message}");
    }

    #[test]
    fn unresolvable_command_in_expression_takes_the_chars_fallback() {
        let st = state_with(&[("frac", vec![brace_arg(), brace_arg()])]);
        let parsed = parse_std(r"\frac{a}\nope", &st, Recovery::Tolerant);

        let frac = root_child(&parsed, 0);
        assert_eq!(frac.span().range(), 0..13);
        let content1 = content_of(frac, 1);
        assert_eq!(content1.len(), 1);
        assert_eq!(content1[0].chars(), Some("\\nope"));
        assert_eq!(parsed.result.diagnostics.len(), 1);
    }

    #[test]
    fn expression_parser_standalone() {
        let st = state_with(&[("e", vec![expression_arg()])]);
        let parsed = parse_both(r"\e {x}y", &st, Recovery::Strict);

        let e = root_child(&parsed, 0);
        assert_eq!(e.span().range(), 0..6);
        // The expression node itself is the content (its braces are part of the value).
        let content = content_of(e, 0);
        assert_eq!(content.len(), 1);
        assert!(content[0].is_group());
        assert_eq!(root_child(&parsed, 1).chars(), Some("y"));

        // Missing expression: diagnosed, absent.
        let parsed = parse_std(r"\e", &st, Recovery::Tolerant);
        let e = root_child(&parsed, 0);
        assert!(!e.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(parsed.result.diagnostics.len(), 1);
        let message = parsed.result.diagnostics.iter().next().unwrap().message().to_string();
        assert!(message.contains("expected an expression"), "unexpected message: {message}");
    }

    // --- state scoping -------------------------------------------------------------------

    #[test]
    fn per_argument_state_delta_scopes_the_arguments_extent() {
        // The argument's delta (comments disabled) is in force for its whole extent —
        // `%b` inside the group is plain chars — and reverted after: ` %c` is a
        // comment again at the sibling level.
        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_comments: Some(false),
            ..TokenRulesOverrides::default()
        });
        let arg = Arc::new(
            ArgumentSpec::new(Arc::new(GroupArgumentParser::new(GT_BRACE)))
                .with_state_delta(delta),
        );
        let st = state_with(&[("m", vec![arg])]);
        let parsed = parse_std("\\m{a%b} %c", &st, Recovery::Strict);

        let m = root_child(&parsed, 0);
        let content = content_of(m, 0);
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].chars(), Some("a%b"));
        assert_eq!(root_child(&parsed, 1).chars(), Some(" "));
        assert!(root_child(&parsed, 2).is_comment());
        assert!(parsed.result.diagnostics.is_empty());
    }

    #[test]
    fn expression_invocation_after_effect_delta_is_dropped() {
        // A takeover parser used *as an argument expression* returns an after-effect
        // delta (comments disabled); the argument has no delta channel, so it must not
        // leak to the enclosing loop: ` %c` is still a comment.
        #[derive(Debug)]
        struct DefSpec;
        impl CallableSpec<ArgLang> for DefSpec {
            fn make_invocation_parser<'a, 's>(
                &'a self,
                invocation: Invocation<'a, 's, ArgLang>,
            ) -> alloc::boxed::Box<dyn ConstructParser<ArgLang, Output = BuildId> + 'a>
            where
                's: 'a,
            {
                struct DefParser<'a, 's> {
                    invocation: Invocation<'a, 's, ArgLang>,
                }
                impl ConstructParser<ArgLang> for DefParser<'_, '_> {
                    type Output = BuildId;
                    fn parse(
                        &mut self,
                        cx: &mut ParseContext<'_, '_, ArgLang>,
                    ) -> ConstructParserResult<
                        ArgLang,
                        (BuildId, Option<ParsingStateDelta<ArgLang>>),
                    > {
                        let span = self.invocation.token.span;
                        let id = cx.session.builder.add(
                            NodeKind::chars(span),
                            SourceSpan::new(&cx.source, span),
                            Arc::clone(&cx.state),
                            vec![],
                        ).unwrap();
                        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
                            enable_comments: Some(false),
                            ..TokenRulesOverrides::default()
                        });
                        Ok((id, Some(delta)))
                    }
                }
                alloc::boxed::Box::new(DefParser { invocation })
            }
        }

        let w: Arc<dyn CallableSpec<ArgLang>> =
            Arc::new(StdCallableSpec::new(vec![brace_arg()]));
        let def: Arc<dyn CallableSpec<ArgLang>> = Arc::new(DefSpec);
        let st = state_with_specs(&[("w", w), ("def", def)]);
        let parsed = parse_std("\\w\\def %c", &st, Recovery::Strict);

        let w = root_child(&parsed, 0);
        let content = content_of(w, 0);
        // The takeover staged its node over the trigger's whole span (post-space
        // bytes included — the token span convention).
        assert_eq!(content[0].chars(), Some("\\def "));
        assert!(root_child(&parsed, 1).is_comment());
        assert!(parsed.result.diagnostics.is_empty());
    }

    // --- the requires_content() guard semantics (slots session, July 2026) --------------

    /// The pylatexenc-parity pin: a callable all of whose arguments can match empty is
    /// *valid* bare in expression position — it dispatches in full (its optional
    /// probed and found absent) instead of taking the old any-declared-arguments
    /// diagnostic. Strict mode passing proves no diagnostic fires at all.
    #[test]
    fn emptiable_arguments_allow_bare_expression_use() {
        // Optional probing is state-dependent: StdTokenReader only.
        let st = state_with(&[("a", vec![brace_arg()]), ("b", vec![optional_arg()])]);
        let parsed = parse_std(r"\a\b x", &st, Recovery::Strict);

        let a = root_child(&parsed, 0);
        let content = content_of(a, 0);
        assert_eq!(content.len(), 1);
        let b = content[0];
        assert_eq!(b.name(), Some("b"));
        // Dispatched in full: the optional argument's entry exists, probed and absent.
        assert!(!b.arguments().unwrap().get(0).unwrap().is_provided());
        assert!(parsed.result.diagnostics.is_empty());

        // ...and when the optional *is* provided, the bare callable swallows it, as in
        // pylatexenc: the expression is the full `\b[x]` invocation.
        let parsed = parse_std(r"\a\b[x]y", &st, Recovery::Strict);
        let b = content_of(root_child(&parsed, 0), 0)[0];
        assert!(b.arguments().unwrap().get(0).unwrap().is_provided());
        assert!(parsed.result.diagnostics.is_empty());
    }

    /// The override channel pin (`\begin`/`\verb` rehearsal): a takeover spec that
    /// declares no arguments but overrides `requires_content()` is diagnosed bare in
    /// expression position and staged as the bare single-token callable — the
    /// deliberate, documented divergence from pylatexenc, which would dispatch the
    /// body-bearing construct as the argument.
    #[test]
    fn requires_content_override_guards_body_bearing_takeovers() {
        #[derive(Debug)]
        struct TakesBodySpec;
        impl CallableSpec<ArgLang> for TakesBodySpec {
            fn requires_content(&self) -> bool {
                true
            }
        }

        let a: Arc<dyn CallableSpec<ArgLang>> =
            Arc::new(StdCallableSpec::new(vec![brace_arg()]));
        let env: Arc<dyn CallableSpec<ArgLang>> = Arc::new(TakesBodySpec);
        let st = state_with_specs(&[("a", a), ("env", env)]);
        let parsed = parse_both(r"\a\env{x}", &st, Recovery::Tolerant);

        // `\a`'s argument is the bare `\env` callable — trigger alone, nothing consumed
        // past it; `{x}` stays sibling content (never handed to `\env`).
        let a = root_child(&parsed, 0);
        let content = content_of(a, 0);
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].name(), Some("env"));
        assert_eq!(content[0].child_count(), 0);
        assert!(root_child(&parsed, 1).is_group());
        assert_eq!(parsed.result.diagnostics.len(), 1);
        let diagnostic = parsed.result.diagnostics.iter().next().unwrap();
        assert_eq!(
            diagnostic.identifier(),
            ExpressionCallableRequiresContent::IDENTIFIER
        );
    }
}
