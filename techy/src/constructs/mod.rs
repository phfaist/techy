//! Construct parsers: the parsing layer of the S1 core.
//!
//! [`ConstructParser`] is the single most important trait in the system: every construct —
//! the main content loop ([`NodesParser`]), groups ([`GroupParser`]), callable invocations
//! ([`StdInvocationParser`], behind the
//! [`make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser) factory),
//! arguments (the standard [`ArgumentParser`](crate::spec::ArgumentParser)
//! implementations), environment bodies ([`EnvironmentBodyParser`]) — is parsed by an
//! implementation of it, reading tokens and staging nodes through one [`ParseContext`].
//!
//! # The two-tier ownership model
//!
//! Construct parsers are **temporaries** (tier 2): constructed with their per-use
//! configuration where they are needed, `parse(&mut self, …)` so working state lives in
//! fields, free to borrow (`'s` content, token refs), dropped when the frame ends —
//! never stored in specs. *Stored* behavior objects (tier 1 — specs,
//! [`ArgumentParser`](crate::spec::ArgumentParser)s) are `Arc`-shared, `Send + Sync`,
//! immutable, and receive every per-use input as arguments. Closures (stop predicates)
//! are thereby confined to tier 2; specs stay data.
//!
//! # State threading (the "caller applies deltas" law, pinned to `cx`)
//!
//! [`ParseContext::state`] is the parser's **input** state — the caller sets it. A parser
//! that scopes a child state (group interior, argument extent, slot body) derives it
//! locally and scopes it through
//! [`parse_construct`](ParseContext::parse_construct) /
//! [`with_parsing_state`](ParseContext::with_parsing_state) (structural revert —
//! `Arc` clone is
//! cheap — plus the session's enclosing-state stack bookkeeping). The
//! `Option<Box<ParsingStateDelta>>` in the
//! return value is exclusively the *after-effect for the caller* (`\newcommand`);
//! it is boxed so the common `None` costs one pointer-sized slot per recursion
//! level, not the full delta struct.
//!
//! # Errors
//!
//! `Err` means **abort**: recovery happens at the detection site (the
//! [`recover`](ParseContext::recover) helper), and abnormal endings of sub-parses travel
//! as data ([`StopCause`]) — nobody continues past an `Err`.

mod argument_parsers;
mod attached_source;
mod chars_group_parser;
mod child_state;
mod embellishments_parser;
mod environment_parser;
mod group_parser;
mod invocation_parser;
mod nodes_parser;
#[cfg(test)]
mod span_tiling_tests;
mod tack_on_parser;
mod verbatim_parser;

pub use argument_parsers::{
    peek_adjacent_argument, scan_argument_noise, stage_pre_space, ArgumentNoise,
    ExpectedExpressionArgument,
    ExpressionParser, GroupArgumentParser, MarkerArgumentParser, MissingMandatoryArgument,
    OptionalGroupArgumentParser,
};
pub use attached_source::{
    AttachedSourceOutcome, InvalidReferenceReason, InvalidSourceReferenceArgument,
    NoSourceResolver, UnresolvableSourceReference,
};
pub use chars_group_parser::CharsGroupArgumentParser;
pub use child_state::{ChildStateSpec, GroupChildState, InvocationChildState};
pub use embellishments_parser::EmbellishmentsArgumentParser;
pub use tack_on_parser::{RepeatedTackOnField, TackOnFieldsArgumentParser};
pub use environment_parser::{
    read_rigid_name_group, EnvironmentBeginSyntaxData, EnvironmentBody,
    EnvironmentBodyParser, EnvironmentTerminatorMismatch,
    EnvironmentTerminatorSyntaxData, MalformedEnvironmentTerminator,
    MissingEnvironmentTerminator, MissingTerminatorFound, NameGroup,
};
pub use group_parser::{
    GroupAfterEffectsFn, GroupParser, UnclosedGroup, UnclosedGroupFound,
};
pub use invocation_parser::{parse_declared_arguments, StdInvocationParser};
pub use nodes_parser::{
    CommandResolutionFailed, ExpressionCallableRequiresContent, NodesOutcome, NodesParser,
    StopCause, StopSpec, StrayGroupClose, TokenStopCondition, TokenStopKind,
    UnresolvableCommand, UnusableRecoveryToken, UnusableRecoveryTokenKind,
};
pub use verbatim_parser::{
    verbatim_state_delta, ExpectedVerbatimDelimiter, UnterminatedVerbatim,
    VerbatimArgumentParser, VerbatimBodyParser, VerbatimBodyTerminator,
};

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::sync::Arc;
use core::fmt;

use crate::engine::{Frame, FrameTitle, ParseDriver, ParserSession, SessionDeriveError};
use crate::error::{Diagnostic, DiagnosticData, DiagnosticInfo, HookFailed, ParseError, Severity};
use crate::source::{SourceSpan, TextContent};
use crate::spec::{CallableSpec, FrameRole};
use crate::node::{
    BuildId, CallableData, NodeBuildError, NodeKind, ParsedArguments, ParsedSlots,
    StagedNodes,
};
use crate::state::{FeaturePresence, Lang, LangFeatures, ParsingState, ParsingStateDelta};
use crate::token::{GroupRule, StreamPosition, Token, TokenEdge, TokenReader};

/// The live frame covering a resolved invocation's parse (the dispatch push site): the spec's title hook with the invocation spelling — the
/// trigger token minus its syntactic post-space — anchored at the trigger. Built before
/// the `Invocation` moves into the spec's parser factory; allocation-free (`Arc` bumps
/// only).
pub(crate) fn invocation_frame<L: Lang>(
    cx: &ParseContext<'_, '_, L>,
    invocation: &Invocation<'_, L>,
) -> Frame<L> {
    let token = invocation.token;
    Frame {
        title: FrameTitle::Callable {
            spec: Arc::clone(invocation.spec),
            role: FrameRole::Invocation,
            name: cx.tokens.source_span_between(token, TokenEdge::Start, TokenEdge::End),
        },
        span: cx.tokens.source_span_of(token),
    }
}

/// Record a spelling fact the reader answered as node data on a node spanning
/// `node_span`: a node-relative [`Span`](crate::source::Span) when the fact lies in
/// the node's own source, the fact's text itself otherwise.
///
/// The one spelling of the node-data rule: a bare span means nothing without the
/// source it indexes, so a fact from another source is recorded as owned text instead
/// of as a span that would read against the wrong content. A fact can only come from
/// another source under a language with
/// [`OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING) `= false`, whose reader
/// may serve one parse from several sources: at a seam between two of them a single
/// token can even have edges in two sources, so one of its sub-spans may lie outside
/// the node's own.
///
/// The rule assumes nothing about the tokens: both arms read the very same
/// reader-answered span, and which arm applies is decided by the source the fact lies
/// in — the residency the all-trees law
/// ([`validate_tree`](crate::node::validate_tree)) checks.
pub(crate) fn node_text_content<O: crate::source::SourceOrigin>(
    fact: &SourceSpan<O>,
    node_span: &SourceSpan<O>,
) -> TextContent {
    match fact.same_source(node_span) {
        true => TextContent::Spanned(fact.span()),
        false => TextContent::Owned(fact.content().into()),
    }
}

/// Append the text of `token`'s pre-space to a run of owned content, as the reader
/// answers it for that token. Does nothing when `text` is `None` — the spelling of "no
/// owned text is being accumulated", which is the case for a language that obeys span
/// tiling ([`Lang::OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING)).
pub(crate) fn push_pre_space_text<L: Lang>(
    text: &mut Option<String>,
    cx: &ParseContext<'_, '_, L>,
    token: &Token<L>,
) {
    let Some(text) = text else { return };
    text.push_str(
        cx.tokens
            .source_span_between(token, TokenEdge::StartBeforePreSpace, TokenEdge::Start)
            .content(),
    );
}

/// Append everything `token` contributes to a run of owned content: its pre-space, the
/// `spelling` the reader classified it as (the character of a
/// [`Char`](crate::token::TokenKind::Char) token, the delimiter of a group close read
/// as content), and its syntactic post-space — three answers about this one token,
/// asked of the reader that produced it.
///
/// This is the recipe multi-token content follows for a language with
/// [`OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING) `= false`, where the
/// tokens of one node need not form one contiguous stretch of one source, so no single
/// span describes their text. Does nothing when `text` is `None` (see
/// [`push_pre_space_text`]).
pub(crate) fn push_token_text<L: Lang>(
    text: &mut Option<String>,
    cx: &ParseContext<'_, '_, L>,
    token: &Token<L>,
    spelling: &str,
) {
    push_pre_space_text(text, cx, token);
    let Some(text) = text else { return };
    text.push_str(spelling);
    text.push_str(
        cx.tokens
            .source_span_between(token, TokenEdge::End, TokenEdge::EndPastPostSpace)
            .content(),
    );
}

/// The `Comment` node kind for a comment token, plus the node's span: the delimiter,
/// the text, and the syntactic post-space, each recorded as node data.
///
/// All four spans are reader answers about the token's edges — the delimiter is
/// `Start..ContentStart`, the content `ContentStart..End`, the post-space
/// `End..EndPastPostSpace`, and the node's span the token's own. The parser computes
/// none of them itself, and records the three sub-spans through the node-data rule
/// ([`node_text_content`]): a span of the node's own source where the answer lies in
/// it, the text itself otherwise — which is what a token whose edges fall in two
/// sources needs (a seam, reachable only under
/// [`Lang::OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING) `= false`).
pub(crate) fn comment_node_kind<L: Lang>(
    cx: &ParseContext<'_, '_, L>,
    token: &Token<L>,
) -> (NodeKind<L>, SourceSpan<L::SourceOrigin>) {
    let span = cx.tokens.source_span_of(token);
    let between = |a, b| cx.tokens.source_span_between(token, a, b);
    let kind = NodeKind::comment(
        node_text_content(&between(TokenEdge::Start, TokenEdge::ContentStart), &span),
        node_text_content(&between(TokenEdge::ContentStart, TokenEdge::End), &span),
        node_text_content(&between(TokenEdge::End, TokenEdge::EndPastPostSpace), &span),
    );
    (kind, span)
}

/// Everything a construct parser needs, in one context value.  Includes pretty much
/// all methods the construct parser might need, including to stage nodes, to delegate
/// parsing to sub-parsers, pushing frames on the frames stack, to change the parsing
/// state, etc.
///
/// **Dispatching fallible public hooks yourself.** A third-party construct parser
/// that consults a fallible hook directly — say
/// [`ParseDriver::resolve_command`](crate::engine::ParseDriver::resolve_command) or
/// [`CallableSpec::make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser)
/// — also owns the error's traceback: the hooks have no session access, so the
/// in-crate consultation sites attach the live traceback to a hook-returned abort
/// error through a crate-internal helper. Do the same at your dispatch site with
/// `error.with_frames(cx.session.snapshot_frames())`
/// ([`ParseError::with_frames`](crate::error::ParseError::with_frames),
/// [`ParserSession::snapshot_frames`](crate::engine::ParserSession::snapshot_frames)),
/// guarded on `error.frames().is_empty()` — an error already carrying frames keeps
/// them.
///
/// **Spans come from the token reader** — the context holds no source handle. A
/// construct parser does not pair a byte range with a source itself: every
/// [`SourceSpan`] it stages or reports is obtained
/// from [`tokens`](ParseContext::tokens) — one token's span
/// ([`TokenReader::source_span_of`](crate::token::TokenReader::source_span_of),
/// [`source_span_between`](crate::token::TokenReader::source_span_between)), the span
/// between two stream positions
/// ([`source_span_within`](ParseContext::source_span_within)), or the empty span at
/// the current position ([`here`](ParseContext::here), the anchor for a condition
/// detected between tokens). Only the reader knows which source it is serving tokens
/// from, so only the reader can answer where a token is.
pub struct ParseContext<'a, 's, L: Lang> {
    /// The token stream — and the answer to every "where?" question: what a token
    /// is worth as a span, and where the stream stands.
    pub tokens: &'a mut dyn TokenReader<'s, L>,
    /// The parser's **input** parsing state (the caller sets it; see the
    /// state-threading contract in [`core::constructs`](crate::core::constructs)).
    pub state: Arc<ParsingState<L>>,
    /// The per-parse [`ParserSession`]. What a construct parser reaches on it
    /// directly: the [`diagnostics`](ParserSession::diagnostics) sink, the
    /// language's session extension ([`ext`](ParserSession::ext)), and a rendered
    /// snapshot of the live frame stack
    /// ([`snapshot_frames`](ParserSession::snapshot_frames)). Everything else a
    /// parser needs from the session goes through this context's own methods,
    /// which are also the preferred spellings for the session-mediated derivations:
    /// staging via [`stage_node`](ParseContext::stage_node) /
    /// [`stage_invocation`](ParseContext::stage_invocation), descents via
    /// [`parse_construct`](ParseContext::parse_construct), state derivation via
    /// [`derive_state`](ParseContext::derive_state) /
    /// [`group_interior_state`](ParseContext::group_interior_state), problem
    /// reporting via [`recover`](ParseContext::recover) /
    /// [`implementation_error`](ParseContext::implementation_error), frames via
    /// [`with_frame`](ParseContext::with_frame). The session's node builder, memo
    /// storage, and frame stack are internal.
    pub session: &'a mut ParserSession<L>,
    /// The language's [`ParseDriver`]: recovery policy, parse-time hooks,
    /// the descent-delta channel, construct provision. **Concretely typed through
    /// `L`** — preset parsers reach preset helper methods (inherent methods on the
    /// driver type) with zero downcasts; generic code sees only the trait.
    pub driver: &'a L::Driver,
}

impl<'a, 's, L: Lang> ParseContext<'a, 's, L> {
    /// Bundle the four parse inputs into a context. Prefer this over a struct literal
    /// (the fields stay public for access): the context is the type's stated "one place
    /// to grow" (depth limits, cancellation), and construction through `new` keeps
    /// future fields from breaking every embedder.
    pub fn new(
        tokens: &'a mut dyn TokenReader<'s, L>,
        state: Arc<ParsingState<L>>,
        session: &'a mut ParserSession<L>,
        driver: &'a L::Driver,
    ) -> ParseContext<'a, 's, L> {
        ParseContext { tokens, state, session, driver }
    }

    /// The empty [`SourceSpan`] at the reader's current stream position — the
    /// diagnostics anchor for a condition detected **between** tokens (a missing
    /// argument, a contract violation with no token to blame).
    pub fn here(&self) -> SourceSpan<L::SourceOrigin> {
        SourceSpan::at(&self.tokens.source_position_at(&self.tokens.position_here()))
    }

    /// The source span running from `begin` to `end` — the span of a construct that
    /// covers several tokens, taken from two stream positions the reader handed out
    /// ([`position_here`](crate::token::TokenReader::position_here),
    /// [`position_at`](crate::token::TokenReader::position_at)).
    ///
    /// The answer follows the language's [`Lang::OBEYS_SPAN_TILING`] declaration:
    ///
    /// - `true` (the default): the span is the range the two positions delimit
    ///   ([`TokenReader::source_span_within`](crate::token::TokenReader::source_span_within)).
    ///   `Err` reports an [`ImplementationError`] when they delimit no such range
    ///   (`end` before `begin`, or the two in different sources) — a contract violation
    ///   by the calling parser or by the reader; it aborts under any recovery policy,
    ///   and never panics.
    /// - `false`: the span is the one the reader *describes* for that stretch of stream
    ///   ([`TokenReader::source_span_describing`](crate::token::TokenReader::source_span_describing)),
    ///   and there is no error case — the two positions need not delimit one range of
    ///   one source, and an inverted pair is not an error either. The returned span is
    ///   a coordinate hint that becomes the node's span: callers must derive no content
    ///   and no structure from it.
    pub fn source_span_within(
        &self,
        begin: &StreamPosition<L>,
        end: &StreamPosition<L>,
    ) -> ConstructParserResult<L, SourceSpan<L::SourceOrigin>> {
        if !L::OBEYS_SPAN_TILING {
            return Ok(self.tokens.source_span_describing(begin, end));
        }
        match self.tokens.source_span_within(begin, end) {
            Some(span) => Ok(span),
            None => Err(self.implementation_error(
                format!(
                    "the positions {begin:?} and {end:?} do not delimit one range of \
                     one source (a span must run forward within a single source)"
                ),
                self.here(),
            )),
        }
    }

    /// Stage one parsed node — **the single staging entry point** of parsing (every
    /// parsed node enters the tree through it), and the one automatic
    /// [`Lang::make_node_ext`] site: the node's ext is minted here (with the
    /// descent-only [`StagedChildren`](crate::node::StagedChildren) view of
    /// `children`), then the node is staged with annotation `()` (parse output is the
    /// unannotated `NodeTree<L>`; annotations are consumer vocabulary).
    ///
    /// Construct parsers have no other route to the builder — no node escapes ext
    /// minting, with zero parser cooperation required. `children` are the node's
    /// structural children in order (for a `Callable`: one region per provided
    /// argument, then one per slot — see
    /// [`NodeTreeBuilder::add`](crate::node::NodeTreeBuilder::add)).
    ///
    /// `Err` reports a staging-contract violation
    /// ([`NodeBuildError`](crate::node::NodeBuildError)) — an implementation bug in an
    /// extension, not a source condition — or the ext mint's own reported failure
    /// ([`ExtMintFailed`](crate::node::NodeBuildError::ExtMintFailed),
    /// [`Lang::make_node_ext`]'s error channel). Lift a contract violation with
    /// [`implementation_error`](ParseContext::implementation_error); lift
    /// `ExtMintFailed` — an operational failure in consumer-supplied hook code, not
    /// a contract violation — as a [`HookFailed`](crate::error::HookFailed)
    /// condition carrying the mint's `detail`, with the live traceback attached
    /// (`error.with_frames(cx.session.snapshot_frames())` when the error carries no
    /// frames yet). The in-crate staging callers match the variant and do exactly
    /// that; either lift aborts the parse under any recovery policy.
    pub fn stage_node(
        &mut self,
        kind: NodeKind<L>,
        span: SourceSpan<L::SourceOrigin>,
        state: Arc<ParsingState<L>>,
        children: Vec<BuildId>,
    ) -> Result<BuildId, NodeBuildError> {
        let ext = L::make_node_ext(
            &kind,
            &span,
            &state,
            self.session.builder.staged_children(&children),
        )?;
        self.session.builder.add(kind, span, state, children, ext, ())
    }

    /// The read-only view of the nodes staged so far, keyed by
    /// [`BuildId`](crate::node::BuildId) — what node-based stop predicates and span
    /// arithmetic over already-staged children consume. Read view only: staging goes
    /// through [`stage_node`](ParseContext::stage_node).
    pub fn staged_nodes(&self) -> StagedNodes<'_, L> {
        self.session.builder.staged_nodes()
    }

    /// Stage the resolved invocation's `Callable` node — the **transcription-case
    /// shorthand** over the single staging entry point ([`stage_node`](ParseContext::stage_node)):
    /// builds the [`CallableData`] by transcribing `callable_type`/`name`/`spec`
    /// from the bundle and minting the invocation-syntax payload from it
    /// ([`FromInvocation`]), computes the node's span, stages (minting the node
    /// ext, as staging always does), and returns the id. What [`StdInvocationParser`] does, packaged
    /// for takeover parsers of the same macro shape.
    ///
    /// `arguments`/`slots` are **caller-tiled** records in staged child-list
    /// coordinates, and `children` the flat child list they tile — the natural
    /// output of the argument loop
    /// ([`parse_declared_arguments`]). The parse-side/restage-side symmetry with
    /// `restage_invocation` is by shared *vocabulary*, deliberately not shared
    /// arity: the restage side passes driver-tiled bundles because the region
    /// arithmetic is owned by the other party there.
    ///
    /// The node's span starts where the trigger starts; where it ends is
    /// decided by `end`, in three cases:
    ///
    /// - `Some(end)`: at that stream position — for takeovers whose consumed extent
    ///   outruns their last child (rest-of-line and heredoc shapes).
    /// - `None` with a last staged child whose span is in the trigger's source: at
    ///   that child's span end (the **standard rule**).
    /// - `None` otherwise (no child, or a child staged in another source): at the
    ///   reader's current stream position — which, for a childless invocation staged
    ///   in the ordinary flow, is just past the trigger's own post-space.
    ///
    /// For a language with
    /// [`OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING) `= false` the last
    /// two cases are one: a staged child's span end says nothing about where the
    /// trigger's own source stands, so the node's span is always the one the reader
    /// describes for the stretch from the trigger's start to where the stream now
    /// stands — just past the last child in the ordinary flow.
    ///
    /// Deliberately **no `callable_type`/`name` overrides**: a composition that
    /// overrides both and whose span outruns its children (the environment shape)
    /// stays on [`stage_node`](ParseContext::stage_node) itself with
    /// an explicit [`CallableData`]. No ext/annotation parameters — staging
    /// mints the ext, and parse annotations are `()`.
    ///
    /// `Err` reports an **invalid computed span** — for a language that obeys span
    /// tiling, a node end (from the standard rule or from `end`) that precedes the
    /// trigger's start, or that lies in another source than the trigger: a contract
    /// violation by the calling parser, lifted as an [`ImplementationError`] that
    /// aborts under any recovery policy, never a panic (a language with
    /// `OBEYS_SPAN_TILING = false` claims nothing about the span and reports no such
    /// error) — or a
    /// [`stage_node`](ParseContext::stage_node) failure lifted per that method's
    /// split (contract violations as [`ImplementationError`], the ext mint's
    /// reported failure as [`HookFailed`](crate::error::HookFailed); both abort
    /// under any recovery policy).
    pub fn stage_invocation(
        &mut self,
        invocation: &Invocation<'_, L>,
        arguments: ParsedArguments<L>,
        slots: ParsedSlots<L>,
        children: Vec<BuildId>,
        end: Option<&StreamPosition<L>>,
    ) -> ConstructParserResult<L, BuildId>
    where
        L::InvocationSyntax: FromInvocation<L>,
    {
        let token = invocation.token;
        let trigger = self.tokens.source_span_of(token);
        let trigger_start = self.tokens.position_at(token, TokenEdge::Start);
        let span = match end {
            // The explicit end: the takeover claims its consumed extent.
            Some(end) => self.invocation_span_within(&trigger, &trigger_start, end)?,
            // The standard rule needs a byte comparison between a child's span and
            // the trigger's, which presupposes span tiling. Where the language does
            // not declare it, no assumption is made about the children at all: the
            // only position this method holds that means "past the last child" is the
            // one the reader stands at (children are staged nodes — spans, not stream
            // positions), so the span is what the reader describes for the stretch
            // from the trigger's start to there.
            None if !L::OBEYS_SPAN_TILING => {
                let here = self.tokens.position_here();
                self.invocation_span_within(&trigger, &trigger_start, &here)?
            }
            None => {
                // The standard rule: the last staged child's span end, in the
                // trigger's own source. A last child no parser ever staged (an
                // implementation bug) falls through as if there were none — the
                // builder diagnoses the foreign id in `add`.
                let child_end = children
                    .last()
                    .and_then(|last| self.staged_nodes().get(*last))
                    .map(|child| child.span().clone())
                    .filter(|child| child.same_source(&trigger))
                    .map(|child| child.end());
                match child_end {
                    Some(child_end) => {
                        // A child ending before the trigger starts is a contract
                        // violation by outer code — the implementation-error abort,
                        // never a panic (`SourceSpan::new` would otherwise assert).
                        if child_end < trigger.start() {
                            return Err(self.invalid_invocation_span(&trigger));
                        }
                        SourceSpan::new(trigger.source(), trigger.start()..child_end)
                    }
                    // No usable child: the node ends where the reader stands.
                    None => {
                        let here = self.tokens.position_here();
                        self.invocation_span_within(&trigger, &trigger_start, &here)?
                    }
                }
            }
        };
        let data = CallableData {
            callable_type: invocation.callable_type,
            name: invocation.name.into(),
            spec: Arc::clone(invocation.spec),
            arguments,
            slots,
            invocation_syntax: L::InvocationSyntax::from_invocation(invocation, &*self.tokens),
        };
        self.stage_node(
            NodeKind::callable(data),
            span.clone(),
            Arc::clone(&self.state),
            children,
        )
        .map_err(|error| self.staging_error(error, span))
    }

    /// [`stage_invocation`](ParseContext::stage_invocation)'s span computation from
    /// two stream positions, with the invalid-span wording (and anchor) of that
    /// method.
    ///
    /// Dispatches on [`Lang::OBEYS_SPAN_TILING`] exactly like
    /// [`source_span_within`](ParseContext::source_span_within): the delimited range
    /// under `true`, the reader's described span (never an error) under `false`.
    fn invocation_span_within(
        &self,
        trigger: &SourceSpan<L::SourceOrigin>,
        begin: &StreamPosition<L>,
        end: &StreamPosition<L>,
    ) -> ConstructParserResult<L, SourceSpan<L::SourceOrigin>> {
        if !L::OBEYS_SPAN_TILING {
            return Ok(self.tokens.source_span_describing(begin, end));
        }
        match self.tokens.source_span_within(begin, end) {
            Some(span) => Ok(span),
            None => Err(self.invalid_invocation_span(trigger)),
        }
    }

    /// The invalid-computed-span abort of
    /// [`stage_invocation`](ParseContext::stage_invocation), anchored at the
    /// trigger — the construct whose staging failed, and the location a reader of
    /// the report can act on.
    fn invalid_invocation_span(
        &self,
        trigger: &SourceSpan<L::SourceOrigin>,
    ) -> ParseError<L::SourceOrigin> {
        self.implementation_error(
            "stage_invocation computed an invalid node span: the node's end must \
             not precede the trigger's start and must lie in the trigger's own \
             source — check the explicit end position or the staged children's \
             spans",
            trigger.clone(),
        )
    }

    /// Probe the token at the current position under `state`, mapping a tokenizer error
    /// per the recovery policy — the **probing peek** of the argument-probe protocol:
    /// strict mode aborts with the token error (mirroring the content loop); tolerant
    /// mode reports `None` **without diagnosing or consuming** — the caller treats the
    /// position as unusable (argument absent, terminator malformed) and the enclosing
    /// content loop re-reads the error and applies its own token recovery, avoiding a
    /// double report.
    ///
    /// A token error carrying **no** recovery is unrecoverable and aborts even under
    /// [`Recovery::Tolerant`](crate::error::Recovery::Tolerant) — mirroring the content loop, whose re-read would abort
    /// anyway; reporting `None` first would only add a spurious absent-position recovery
    /// (and its diagnostic) on the way down.
    ///
    /// `state` is passed explicitly (usually `&Arc::clone(&cx.state)`; peeking never
    /// consumes) so a parser can probe under a derived state — an optional-argument
    /// parser peeking with its minted group rule in force — without swapping
    /// [`state`](ParseContext::state).
    ///
    /// Thin sugar over [`ParseDriver::probe_token`], where the policy is defined.
    pub fn probe_token(
        &mut self,
        state: &Arc<ParsingState<L>>,
    ) -> ConstructParserResult<L, Option<Token<L>>> {
        self.driver.probe_token(self.tokens, self.session, state)
    }

    /// Run `parser` as one **sub-parse** — the single entry point that every descent
    /// (a construct parser running another [`ConstructParser`] over the same input)
    /// MUST go through; the pylatexenc
    /// `walker.parse_content(parser, …, parsing_state)` analog.
    ///
    /// The contract is **normative**: a `ConstructParser` runs only through this
    /// method — called directly, or through the thin wrappers
    /// [`parse_nodes`](ParseContext::parse_nodes) and
    /// [`parse_group`](ParseContext::parse_group), which delegate here. One shared
    /// entry point is what lets the engine attach its per-descent bookkeeping (the
    /// enclosing-state stack, the optional traceback frame, the
    /// [`DescentGuard`](crate::engine::DescentGuard) depth check) uniformly, with
    /// no per-site cooperation. The contract's limit: plain Rust recursion that
    /// bypasses it — code calling another parser's
    /// [`parse`](ConstructParser::parse) method directly — cannot be detected by
    /// the library. The rule is documented, not enforceable.
    ///
    /// Before the sub-parse runs, the session's per-parse
    /// [`DescentGuard`](crate::engine::DescentGuard) is asked whether the parse
    /// may go one level deeper: a warning answer is recorded as a
    /// [`DescentLimitApproaching`] diagnostic and the parse continues; a refusal
    /// aborts with a [`DescentLimitExceeded`] error under **any** recovery policy
    /// (past the limit there is no safe way to continue).
    ///
    /// `state` is the sub-parse's input state, scoped structurally for the duration
    /// of the run — swapped in, restored afterwards, with the session's
    /// enclosing-state stack maintained alongside (the
    /// [`with_parsing_state`](ParseContext::with_parsing_state) discipline):
    ///
    /// - `Some(state)`: the sub-parse runs under `state`.
    /// - `None`: the sub-parse runs under the **current** state — exactly as if
    ///   `Some(Arc::clone(&cx.state))` had been passed: the same swap/restore
    ///   scoping runs and the same enclosing-state stack entry is pushed. `None`
    ///   never means "skip the scoping".
    ///
    /// `frame`, when `Some`, is pushed on the session's live frame stack around the
    /// whole sub-parse (the [`with_frame`](ParseContext::with_frame) discipline):
    /// every condition recorded while `parser` runs carries the frame in its
    /// traceback snapshot. The frame is popped before this method returns, on the
    /// `Ok` and `Err` paths alike (errors are values, not unwinds).
    ///
    /// The returned [`ParsingStateDelta`] is the construct's after-effect for the
    /// caller, passed through **unapplied** — whether and where it applies is
    /// caller business (the "caller applies deltas" law).
    // The return type is the `ConstructParser::parse` shape spelled out; an alias for
    // it would add a second public name for one method's tuple.
    #[allow(clippy::type_complexity)]
    pub fn parse_construct<P>(
        &mut self,
        parser: &mut P,
        state: Option<Arc<ParsingState<L>>>,
        frame: Option<Frame<L>>,
    ) -> ConstructParserResult<L, (P::Output, Option<Box<ParsingStateDelta<L>>>)>
    where
        P: ConstructParser<L> + ?Sized,
    {
        // `None` = the current state; the scoping below runs identically either way.
        let state = match state {
            Some(state) => state,
            None => Arc::clone(&self.state),
        };
        let framed = frame.is_some();
        if let Some(frame) = frame {
            self.session.push_frame(frame);
        }
        // The descent guard: ask the session's per-parse guard whether one more
        // level may open. A refusal aborts under ANY recovery policy — past the
        // limit there is no safe way to continue — with the just-pushed frame in
        // the error's traceback, and popped again before returning.
        match self.session.enter_descent() {
            Ok(None) => {}
            Ok(Some(warning)) => {
                // The guard's early warning (the standard guard's one-time
                // half-budget notice under the unconfigured default): recorded
                // immediately, at the current position, with the live traceback.
                let span = self.here();
                let frames = self.session.snapshot_frames();
                self.session.diagnostics.push(Diagnostic::from_parts(
                    Severity::Warning,
                    Box::new(DescentLimitApproaching::new(warning.detail)),
                    span,
                    frames,
                ));
            }
            Err(refusal) => {
                let error = ParseError::new(
                    DescentLimitExceeded::new(refusal.detail),
                    self.here(),
                )
                .with_frames(self.session.snapshot_frames());
                if framed {
                    self.session.pop_frame();
                }
                return Err(error);
            }
        }
        let result = self.with_parsing_state(state, |cx| parser.parse(cx));
        // The guard exit runs on the `Ok` and `Err` result paths alike — errors
        // are values, not unwinds — and only for a granted entry (a refusal
        // returned above, before any descent opened).
        self.session.exit_descent();
        // The pop covers the `Err` path too — errors are values, not unwinds.
        if framed {
            self.session.pop_frame();
        }
        result
    }

    /// Run `f` with [`state`](ParseContext::state) scoped to `state`, restoring the
    /// outer state afterwards — the closure-shaped scoped-state primitive under
    /// [`parse_construct`](ParseContext::parse_construct), for state scopes that
    /// are not `ConstructParser`-shaped (the per-argument delta around
    /// `ArgumentParser::parse_argument`; takeover parsers scoping hand-derived
    /// states around arbitrary code).
    ///
    /// This is a **state-scoping utility, not a descent entry point**: code that
    /// runs another [`ConstructParser`] must go through
    /// [`parse_construct`](ParseContext::parse_construct) (which performs this
    /// scoping itself, plus its per-descent obligations), never through this
    /// method alone.
    ///
    /// Besides the structural swap/restore, this maintains the session's
    /// **enclosing-state stack** ([`ParsingStateStack`](crate::state::ParsingStateStack)):
    /// `state` is pushed for the duration of `f` and popped after — the same
    /// closure-scoped discipline as [`with_frame`](ParseContext::with_frame), at
    /// the same descent points. Prefer this over hand-rolled `cx.state` swaps: a
    /// manual swap leaves the stack (and therefore context-dependent event
    /// lowering, [`derive_state`](ParseContext::derive_state)) blind to the scope.
    pub fn with_parsing_state<R>(
        &mut self,
        state: Arc<ParsingState<L>>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.session.push_state(Arc::clone(&state));
        let outer = core::mem::replace(&mut self.state, state);
        let result = f(self);
        self.state = outer;
        self.session.pop_state();
        result
    }

    /// Detection-site recovery — **the recovery entry point**: every problem a
    /// construct parser detects in the source is reported through this one method,
    /// which boxes the condition and hands it to [`ParseDriver::recover`], where the
    /// policy is defined — the default driver path applies
    /// [`refine_diagnostic`](ParseDriver::refine_diagnostic) exactly once (it needs
    /// this context's parsing state) and then records the condition as an
    /// error-severity diagnostic and returns `Ok(())` (tolerant — the caller continues
    /// with its local recovery) or returns it as a [`ParseError`] to bubble (strict).
    pub fn recover(
        &mut self,
        condition: impl DiagnosticInfo,
        span: SourceSpan<L::SourceOrigin>,
    ) -> Result<(), ParseError<L::SourceOrigin>> {
        self.recover_boxed(Box::new(condition), span)
    }

    /// Attach the live traceback to a hook-returned abort error that carries no
    /// frames of its own — extension hooks (driver hooks, descent-state
    /// callbacks) have no session access, so the call site is where the snapshot
    /// exists. An error already carrying frames passes through unchanged.
    pub(crate) fn attach_hook_frames(
        &self,
        error: ParseError<L::SourceOrigin>,
    ) -> ParseError<L::SourceOrigin> {
        if error.frames().is_empty() {
            error.with_frames(self.session.snapshot_frames())
        } else {
            error
        }
    }

    /// The recovery entry point's boxed form — for payloads that already live behind the dyn facade
    /// (the token-error lift, where a `Custom` payload must not be double-boxed).
    pub(crate) fn recover_boxed(
        &mut self,
        data: Box<dyn DiagnosticData>,
        span: SourceSpan<L::SourceOrigin>,
    ) -> Result<(), ParseError<L::SourceOrigin>> {
        self.driver.recover(self.session, &self.state, data, span)
    }

    /// The parser-facing state derivation, from the **current** state
    /// ([`state`](ParseContext::state)) — the one derivation point every construct
    /// parser derives through: lowers **context-dependent events**, then runs the
    /// session-mediated derivation ([`ParserSession::derived_state`] with this
    /// context's driver), so every transition reaches
    /// [`ParseDriver::observe_transition`]. Deriving from another base goes through
    /// the session method directly
    /// (`cx.session.derived_state(cx.driver, &base, &delta)`) — with no event
    /// lowering: context-dependent events are positional, they mean something only
    /// at *this* context's position.
    ///
    /// # Event lowering
    ///
    /// When the delta carries [`events`](crate::state::ParsingStateDelta::events),
    /// each is offered to [`ParseDriver::resolve_state_event`] with the session's
    /// live enclosing-state stack (current state first —
    /// [`ParsingStateStack`](crate::state::ParsingStateStack)). Lowered events are
    /// **removed** and their patches merged; unlowered (context-free) events stay
    /// for [`Lang::finalize_transition`](crate::state::Lang::finalize_transition).
    /// The event *loop* lives here — parsers never iterate events; per-event
    /// *policy* lives on the driver. Merge order: patches apply in event order,
    /// and the delta's own explicit overrides win over patch overrides (the delta
    /// author spoke).
    ///
    /// # Failures
    ///
    /// **Failing scope ops** are reported through the recovery entry point as
    /// [`ScopeOpFailed`] conditions at the current position: under
    /// [`Recovery::Strict`](crate::error::Recovery::Strict) the first failure aborts;
    /// under [`Recovery::Tolerant`](crate::error::Recovery::Tolerant) each failure is
    /// recorded and the parse continues under the error's
    /// [`recovered`](crate::state::DeriveError::recovered) state (the failing ops
    /// skipped, everything else applied), with the transition observed as usual.
    /// A **finalize refusal** ([`FinalizeError`](crate::state::FinalizeError) — a
    /// context-requiring event the driver did not lower) is an extension wiring
    /// bug, not a source condition: it aborts as an
    /// [`ImplementationError`] under any recovery policy. An `Err` from the
    /// lowering hook itself
    /// ([`ParseDriver::resolve_state_event`]'s abort channel) propagates
    /// unchanged, aborting under any recovery policy.
    pub fn derive_state(
        &mut self,
        delta: &ParsingStateDelta<L>,
    ) -> ConstructParserResult<L, Arc<ParsingState<L>>> {
        let base = Arc::clone(&self.state);
        let effective;
        let delta = if delta.events.is_empty() {
            delta
        } else {
            effective = self.lower_state_events(delta)?;
            &effective
        };
        self.commit_derivation(&base, delta)
    }

    /// [`derive_state`](ParseContext::derive_state) with the **effective, as-applied
    /// delta recorded**: after the transition commits (cleanly or through the
    /// tolerant ops-skipped recovery), the delta actually handed to the derivation —
    /// context-dependent events already lowered into their override patches — is
    /// merged into `record` ([`ParsingStateDelta::merge_from`]: later field
    /// overrides win, scope ops and events concatenate in application order). This
    /// is where the merged after-effect record
    /// ([`NodesOutcome::after_effects`]) is captured.
    ///
    /// Recording the *effective* delta is essential: context-dependent events
    /// are positional (they mean something only at this context's position), so a
    /// record replayed elsewhere must carry their lowered patches, never the raw
    /// events. Context-free events that survive lowering are recorded as-is — by
    /// the [`Lang::Event`](crate::state::Lang::Event) contract they are consumed
    /// wherever the delta is applied, so they replay exactly. On the tolerant
    /// ops-skipped path the record keeps the delta **as applied** (the
    /// [`DeriveError::delta`](crate::state::DeriveError::delta) notion), failing
    /// ops included: a replay elsewhere re-attempts them and may re-diagnose.
    pub(crate) fn derive_state_recording(
        &mut self,
        delta: &ParsingStateDelta<L>,
        record: &mut Option<Box<ParsingStateDelta<L>>>,
    ) -> ConstructParserResult<L, Arc<ParsingState<L>>> {
        let base = Arc::clone(&self.state);
        let effective;
        let applied = if delta.events.is_empty() {
            delta
        } else {
            effective = self.lower_state_events(delta)?;
            &effective
        };
        let new = self.commit_derivation(&base, applied)?;
        match record {
            Some(record) => record.merge_from(applied.clone()),
            None => *record = Some(Box::new(applied.clone())),
        }
        Ok(new)
    }

    /// The shared commit tail of the derivation seams: run the session-mediated
    /// derivation (so the transition reaches
    /// [`ParseDriver::observe_transition`]) and route derivation failures through
    /// [`recover_derive_failure`](ParseContext::recover_derive_failure). An
    /// observation abort ([`SessionDeriveError::Observe`] —
    /// [`ParseDriver::observe_transition`]'s `Err`) propagates unchanged under
    /// any recovery policy, with the live traceback attached here.
    fn commit_derivation(
        &mut self,
        base: &Arc<ParsingState<L>>,
        applied: &ParsingStateDelta<L>,
    ) -> ConstructParserResult<L, Arc<ParsingState<L>>> {
        match self.session.derived_state(self.driver, base, applied) {
            Ok(new) => Ok(new),
            Err(SessionDeriveError::Derive(failure)) => {
                self.recover_derive_failure(base, failure)
            }
            Err(SessionDeriveError::Observe(error)) => Err(self.attach_hook_frames(error)),
        }
    }

    /// Derive via [`derive_state`](ParseContext::derive_state), then run `f` with
    /// [`state`](ParseContext::state) scoped to the derived state
    /// ([`with_parsing_state`](ParseContext::with_parsing_state)) — the
    /// delta-shaped state scope in one call, for takeover parsers.
    ///
    /// Like `with_parsing_state`, this is a state-scoping utility, **not** a
    /// descent entry point: running another [`ConstructParser`] under the derived
    /// state goes through [`parse_construct`](ParseContext::parse_construct)
    /// instead.
    pub fn with_derived_state<R>(
        &mut self,
        delta: &ParsingStateDelta<L>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> ConstructParserResult<L, R> {
        let state = self.derive_state(delta)?;
        Ok(self.with_parsing_state(state, f))
    }

    /// The event loop of [`derive_state`](ParseContext::derive_state): offer each
    /// event to the driver with the live enclosing-state stack lent **current
    /// state first** (the context's current state is pushed for the lend when it
    /// is not already the innermost entry — sibling after-effects evolve
    /// `cx.state` between descents; an `Arc`-equal duplicate is harmless under
    /// the stack's scan semantics), and build the effective delta: patches merged
    /// in event order, the original delta's explicit overrides on top, lowered
    /// events removed. A hook `Err` aborts ([`ParseDriver::resolve_state_event`]'s
    /// contract), with the lent stack entry popped and the live traceback
    /// attached.
    fn lower_state_events(
        &mut self,
        delta: &ParsingStateDelta<L>,
    ) -> ConstructParserResult<L, ParsingStateDelta<L>> {
        let lent = match self.session.state_stack().innermost() {
            Some(innermost) => !Arc::ptr_eq(innermost, &self.state),
            None => true,
        };
        if lent {
            self.session.push_state(Arc::clone(&self.state));
        }
        let mut patches: Vec<ParsingStateDelta<L>> = Vec::new();
        let mut kept_events: Vec<L::Event> = Vec::new();
        let mut abort: Option<ParseError<L::SourceOrigin>> = None;
        for event in &delta.events {
            match self.driver.resolve_state_event(event, self.session.state_stack()) {
                Ok(Some(patch)) => patches.push(patch),
                Ok(None) => kept_events.push(event.clone()),
                Err(error) => {
                    // Break, don't return: the lent stack entry pops on the
                    // abort path too.
                    abort = Some(error);
                    break;
                }
            }
        }
        if lent {
            self.session.pop_state();
        }
        if let Some(error) = abort {
            return Err(self.attach_hook_frames(error));
        }

        // Merge: patches in event order (later wins), then the original delta's own
        // explicit overrides on top of all patches (the delta author spoke).
        let mut effective = ParsingStateDelta::new();
        for patch in patches {
            effective.rules.merge_from(patch.rules);
            // Matched store projections: with the scopes feature absent, neither
            // side's zero-sized scope-op store holds anything to concatenate.
            if let (Some(ops), Some(patch_ops)) = (
                <L::Features as LangFeatures>::Scopes::store_get_mut(&mut effective.scope_ops),
                <L::Features as LangFeatures>::Scopes::store_into_inner(patch.scope_ops),
            ) {
                ops.extend(patch_ops);
            }
            if patch.mode.is_some() {
                effective.mode = patch.mode;
            }
            if patch.ext.is_some() {
                effective.ext = patch.ext;
            }
            // Patch events are context-free by contract (resolve_state_event docs):
            // they pass through to finalize_transition un-lowered.
            effective.events.extend(patch.events);
        }
        effective.rules.merge_from(delta.rules.clone());
        if let (Some(ops), Some(delta_ops)) = (
            <L::Features as LangFeatures>::Scopes::store_get_mut(&mut effective.scope_ops),
            <L::Features as LangFeatures>::Scopes::store_get(&delta.scope_ops),
        ) {
            ops.extend(delta_ops.iter().cloned());
        }
        if delta.mode.is_some() {
            effective.mode = delta.mode;
        }
        if delta.ext.is_some() {
            effective.ext = delta.ext.clone();
        }
        effective.events.extend(kept_events);
        Ok(effective)
    }

    /// The group-interior derivation from the **current** state — sugar over
    /// [`ParserSession::group_interior_state`] supplying this context's driver: the
    /// canonical expecting-close override merged with the driver's
    /// [`group_interior_delta`](ParseDriver::group_interior_delta), memoized per
    /// `(base, rule)`. Failing scope ops in the driver's descent delta recover exactly
    /// like [`derive_state`](ParseContext::derive_state)'s (the recovered interior
    /// still expects the entered rule's close — the descent invariant is an override,
    /// not an op).
    pub fn group_interior_state(
        &mut self,
        rule: &Arc<GroupRule<L>>,
    ) -> ConstructParserResult<L, Arc<ParsingState<L>>> {
        let base = Arc::clone(&self.state);
        match self.session.group_interior_state(self.driver, &base, rule) {
            Ok(new) => Ok(new),
            Err(SessionDeriveError::Derive(failure)) => {
                self.recover_derive_failure(&base, failure)
            }
            Err(SessionDeriveError::Observe(error)) => Err(self.attach_hook_frames(error)),
        }
    }

    /// The shared recovery path of the two fallible derivation sugars: report every
    /// failing op through the recovery entry point (strict: the first one aborts); a
    /// finalize refusal aborts as an [`ImplementationError`] under any policy (a
    /// context-requiring event reached the underlying derivation point un-lowered —
    /// the driver failed to lower it: extension wiring, not source input). Otherwise commit
    /// the ops-skipped transition — continue under the error's recovered state
    /// and observe it with the delta the derivation actually applied (which the
    /// error carries: for group interiors that is the *merged* descent delta this
    /// context never built).
    fn recover_derive_failure(
        &mut self,
        base: &Arc<ParsingState<L>>,
        failure: crate::state::DeriveError<L>,
    ) -> ConstructParserResult<L, Arc<ParsingState<L>>> {
        let span = self.here();
        let crate::state::DeriveError {
            failures,
            finalize_error,
            recovered,
            delta,
        } = failure;
        for failed_op in &failures {
            self.recover(ScopeOpFailed::new(failed_op.to_string()), span.clone())?;
        }
        if let Some(finalize_error) = finalize_error {
            return Err(self.implementation_error(finalize_error, span));
        }
        // Tolerant continuation: commit the recovered transition — the session seam
        // observed nothing on the Err path (no transition had been committed). The
        // observation hook's own abort channel applies here like everywhere else.
        let recovered = Arc::new(recovered);
        self.driver
            .observe_transition(
                &mut self.session.ext,
                &mut self.session.diagnostics,
                base,
                &recovered,
                &delta,
            )
            .map_err(|error| self.attach_hook_frames(error))?;
        Ok(recovered)
    }

    /// **A thin wrapper over [`parse_construct`](ParseContext::parse_construct)**:
    /// builds the content-loop parser from the driver's
    /// [`make_nodes_parser`](ParseDriver::make_nodes_parser) factory and runs it
    /// through `parse_construct(parser, Some(state), None)`. Fusing the factory
    /// with the descent entry point is the point of this method: routing every
    /// content descent through it makes one driver override apply at every
    /// descent site.
    ///
    /// Parses one **nodes descent** (a content run: group interior, environment
    /// body, top-level drive) under `state`. State scoping and restoration follow
    /// [`parse_construct`](ParseContext::parse_construct).
    ///
    /// # Resuming a stopped run
    ///
    /// A run that stops before end of input — a token condition, an unexpected group
    /// close ([`StopCause`]) — can be **resumed**: handle the stop (diagnose, consume
    /// or skip the offending token), then call `parse_nodes` again. There is no resume
    /// method on [`NodesParser`]; resumption is *re-invocation* — each call builds a
    /// fresh parser from the factory with its own per-run `stop`/`child_states` — and
    /// the caller bridges the runs. The canonical bridge is the root drive loop
    /// ([`Language::parse_source`](crate::engine::Language::parse_source)), which
    /// diagnoses a stray group close, skips it, and re-enters. The bridge has three
    /// obligations:
    ///
    /// - **Resume under [`NodesOutcome::state`], never under this context's restored
    ///   `state`.** The descent restores [`state`](ParseContext::state) structurally on
    ///   return ([`parse_construct`](ParseContext::parse_construct)), and the run's sibling
    ///   after-effects (a `\newcommand`-style definition) are applied *internally*, not
    ///   returned as a pass-through delta — so after the call, the outcome's exported
    ///   live state is their only carrier. Re-entering with a clone of `cx.state`
    ///   silently rolls them back. A caller that re-anchors its ambient state first
    ///   (the root loop's `cx.state = outcome.state`) makes the two coincide.
    ///
    /// - **Stand the reader where the next run should start.** The stop contract is
    ///   defined ([`TokenStopCondition::consume`], [`StopCause`]'s per-variant docs): a
    ///   left stop token sits at its own start, pre-space already staged, and
    ///   re-peeks clean — but re-entering with the reader still on it and the same
    ///   stop condition stops again immediately, staging nothing: an infinite loop for
    ///   an unconditional resumer. Deal with the token first — consume it
    ///   ([`probe_token`](ParseContext::probe_token) +
    ///   [`move_to`](crate::token::TokenReader::move_to), the environment
    ///   terminator flow), skip it
    ///   ([`move_to_position`](crate::token::TokenReader::move_to_position) with the
    ///   stop cause's `after` position, the root loop), or exclude it from the next
    ///   run's stop spec.
    ///
    /// - **Concatenate the segments yourself.** Each run returns its own
    ///   [`NodesOutcome::nodes`]; the resuming caller extends one list across runs.
    ///   Bytes skipped *between* segments then belong to no node — either accept the
    ///   diagnosed byte-accounting break (the root loop's stray-close precedent) or
    ///   stage a span-backed chars fallback over them (the unresolvable-command
    ///   precedent, [`UnresolvableCommand`]) so the siblings keep tiling the parsed
    ///   extent. And a node stop condition counts each run's own siblings — its count
    ///   argument restarts at zero in a resumed segment. A bridge that *propagates*
    ///   the runs' state effects likewise merges each run's
    ///   [`NodesOutcome::after_effects`] into one record in run order (the
    ///   precedent of
    ///   [`parse_attached_source`](ParseContext::parse_attached_source)).
    ///
    /// Whether to resume at all is a per-construct policy question, not a default:
    /// the environment body deliberately **unwinds** on a terminator mismatch instead
    /// of resuming — a body that diagnosed `\end{A}` and kept going inside
    /// `\begin{A}…\begin{B}…\end{A}` would consume the enclosing environment's
    /// terminator (see [`EnvironmentBodyParser`]).
    // The output-plus-delta pair is the decided ConstructParser signature ([§dd-dr:parsers-engine]).
    #[allow(clippy::type_complexity)]
    pub fn parse_nodes<'p>(
        &mut self,
        state: Arc<ParsingState<L>>,
        stop: StopSpec<'p, L>,
        child_states: ChildStateSpec<'p, L>,
    ) -> ConstructParserResult<L, (NodesOutcome<L>, Option<Box<ParsingStateDelta<L>>>)>
    where
        'a: 'p,
        L::InvocationSyntax: FromInvocation<L>,
    {
        let driver = self.driver;
        // A factory Err aborts under any policy ("could not build the parser" —
        // the hook fallibility contract), with the live traceback attached here.
        let mut parser = driver
            .make_nodes_parser(stop, child_states)
            .map_err(|error| self.attach_hook_frames(error))?;
        self.parse_construct(&mut *parser, Some(state), None)
    }

    /// **A thin wrapper over [`parse_construct`](ParseContext::parse_construct)**:
    /// builds the group parser from the driver's
    /// [`make_group_parser`](ParseDriver::make_group_parser) factory and runs it
    /// through `parse_construct(parser, Some(base), frame)`. Fusing the factory
    /// with the descent entry point is the point of this method: routing every
    /// group descent through it makes one driver override apply at every group
    /// site.
    ///
    /// Parses one **group descent** (the consumed `GroupOpen` token and its resolved
    /// rule) with `base` as the group's input state. `frame`, when `Some`, is pushed
    /// around the whole descent
    /// ([`parse_construct`](ParseContext::parse_construct)'s frame semantics).
    ///
    /// The returned delta is the group's **after-effect for the caller** — `None` for the
    /// standard group, which drops its interior's state changes with the descent, and
    /// `Some` only where the language installed the leak hook
    /// ([`GroupAfterEffectsFn`], the `\gdef` shape). A caller with an after-effect channel
    /// of its own (the content loop) applies it; one without (the argument parsers) drops
    /// it.
    // Same decided pair as above.
    #[allow(clippy::type_complexity)]
    pub fn parse_group<'p>(
        &mut self,
        base: Arc<ParsingState<L>>,
        open: &Token<L>,
        rule: Arc<GroupRule<L>>,
        child_states: ChildStateSpec<'p, L>,
        frame: Option<Frame<L>>,
    ) -> ConstructParserResult<L, (BuildId, Option<Box<ParsingStateDelta<L>>>)>
    where
        'a: 'p,
        L::InvocationSyntax: FromInvocation<L>,
    {
        let driver = self.driver;
        // A factory Err aborts under any policy, as in `parse_nodes`.
        let mut parser = driver
            .make_group_parser(open, rule, child_states)
            .map_err(|error| self.attach_hook_frames(error))?;
        self.parse_construct(&mut *parser, Some(base), frame)
    }

    /// Run `f` with `frame` pushed on the session's live frame stack — the descent-point
    /// primitive of the parse traceback: every condition recorded through the
    /// recovery entry point while `f` runs carries the frame in its snapshot.
    ///
    /// Closure-scoped rather than an RAII guard, deliberately: a guard would hold
    /// `&mut self` against the parser body. The pop after `f` returns covers the `Err`
    /// path too — a bubbling [`ParseError`] leaves no stale frames behind.
    pub fn with_frame<R>(&mut self, frame: Frame<L>, f: impl FnOnce(&mut Self) -> R) -> R {
        self.session.push_frame(frame);
        let result = f(self);
        self.session.pop_frame();
        result
    }

    /// Build the abort error for an extension-implementation contract violation
    /// detected at `span` — an [`ImplementationError`] with the live traceback
    /// attached. `detail` is typically a [`NodeBuildError`](crate::node::NodeBuildError)
    /// or a literal contract description.
    ///
    /// Deliberately **not** the recovery entry point: an implementation bug is not a source
    /// condition — it aborts even under [`Recovery::Tolerant`](crate::error::Recovery::Tolerant), and no
    /// [`ParseDriver::refine_diagnostic`] pass applies.
    pub fn implementation_error(
        &self,
        detail: impl fmt::Display,
        span: SourceSpan<L::SourceOrigin>,
    ) -> ParseError<L::SourceOrigin> {
        ParseError::new(ImplementationError::new(detail.to_string()), span)
            .with_frames(self.session.snapshot_frames())
    }

    /// Build the abort error for a failed staging call — the in-crate lift of a
    /// [`NodeBuildError`](crate::node::NodeBuildError) detected at `span`, applying
    /// the condition split stated on [`stage_node`](ParseContext::stage_node): the
    /// ext mint's own reported failure
    /// ([`ExtMintFailed`](crate::node::NodeBuildError::ExtMintFailed)) becomes a
    /// [`HookFailed`] condition carrying the mint's `detail` — an operational
    /// failure in consumer-supplied hook code, not a contract violation — and every
    /// other variant goes through
    /// [`implementation_error`](ParseContext::implementation_error). Both arms
    /// attach the live traceback and abort under any recovery policy.
    pub(crate) fn staging_error(
        &self,
        error: NodeBuildError,
        span: SourceSpan<L::SourceOrigin>,
    ) -> ParseError<L::SourceOrigin> {
        match error {
            NodeBuildError::ExtMintFailed { detail } => {
                ParseError::new(HookFailed::new(detail, None), span)
                    .with_frames(self.session.snapshot_frames())
            }
            other => self.implementation_error(other, span),
        }
    }
}

/// Condition: an implementation of an extension point — an argument or construct
/// parser, a [`Lang`] hook, a spec factory — violated a library contract. An
/// implementation bug to fix, not a source-input problem: it aborts the parse even
/// under [`Recovery::Tolerant`](crate::error::Recovery::Tolerant) (built through
/// [`ParseContext::implementation_error`], which ignores the recovery policy).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.constructs.implementation-error",
    message = "implementation error (extension contract violation): {detail}"
)]
pub struct ImplementationError {
    /// Description of the violated contract.
    pub detail: String,
}

/// Condition: a scope op of an in-parse state delta failed
/// ([`ScopeOpError`](crate::scopes::ScopeOpError), rendered into `detail`) — reported
/// through the recovery entry point by the [`ParseContext`] derivation sugars: strict parses abort on it; tolerant parses record it and
/// continue under the ops-skipped state
/// ([`DeriveError::recovered`](crate::state::DeriveError::recovered)).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.specs.scope-op-failed",
    message = "scope op failed: {detail}"
)]
pub struct ScopeOpFailed {
    /// The rendered failure ([`ScopeOpError`](crate::scopes::ScopeOpError)'s
    /// `Display`).
    pub detail: String,
}

/// Condition: the parse's [`DescentGuard`](crate::engine::DescentGuard) refused to
/// open one more nesting level — the input nests deeper than the configured (or
/// built-in default) limit allows. Raised by
/// [`ParseContext::parse_construct`](ParseContext::parse_construct) and **aborts
/// under any recovery policy**: the limit exists so deeply nested input cannot
/// crash the process by stack exhaustion, and past it there is no safe way to
/// continue. The `detail` names the limit that was hit and, when the parse ran on
/// the unconfigured built-in default, points at
/// [`Language::with_descent_guard_init`](crate::engine::Language::with_descent_guard_init).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.constructs.descent-limit-exceeded",
    message = "the input nests too deeply to parse: {detail}"
)]
pub struct DescentLimitExceeded {
    /// The guard's explanation: which limit was hit, and how to configure it.
    pub detail: String,
}

/// Condition (warning severity): the parse's
/// [`DescentGuard`](crate::engine::DescentGuard) reported the nesting limit as
/// getting close — for the standard guard
/// ([`StdDescentGuard`](crate::engine::StdDescentGuard)): the first descent past
/// half of the **unconfigured built-in default** stack budget, reported once per
/// parse. Recorded immediately by
/// [`ParseContext::parse_construct`](ParseContext::parse_construct); the parse
/// continues. A configured limit never produces this warning — it exists as the
/// early notice that no limit was chosen explicitly and that the built-in default
/// will soon refuse deeper input; choose a limit with
/// [`Language::with_descent_guard_init`](crate::engine::Language::with_descent_guard_init).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.constructs.descent-limit-approaching",
    message = "the input's nesting approaches the parsing depth limit: {detail}"
)]
pub struct DescentLimitApproaching {
    /// The guard's explanation: how close the limit is, and how to configure it.
    pub detail: String,
}

impl<L: Lang> fmt::Debug for ParseContext<'_, '_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParseContext")
            .field("at", &self.here())
            .field("state", &self.state)
            .field("session", &self.session)
            .finish()
    }
}

/// Result type of construct parsing. `Err` means abort — see the error contract
/// in [`core::constructs`](crate::core::constructs).
///
/// Parameter convention follows [`TokenResult`](crate::token::TokenResult) (lang first,
/// payload last). The underlying [`ParseError`] is generic over the source origin only
/// (mirroring [`Diagnostic`](crate::error::Diagnostic)); the alias derives it from `L`.
pub type ConstructParserResult<L, T> = Result<T, ParseError<<L as Lang>::SourceOrigin>>;

/// A parser for one construct, reading tokens and staging nodes through the context.
///
/// Implementations are tier-2 **temporaries** (the two-tier ownership model in
/// [`core::constructs`](crate::core::constructs)): per-use configuration in
/// fields, `&mut self` working state, dropped with the frame.
///
/// On success, a parser returns its output (typically staged `BuildId`s) together with
/// an optional boxed [`ParsingStateDelta`] — the construct's *after-effect for the
/// caller* (`\newcommand` pushing definitions for subsequent siblings), never its
/// internal scoping. The delta is boxed so the common `None` case costs one
/// pointer-sized return slot per recursion level, not the full delta struct.
pub trait ConstructParser<L: Lang> {
    /// What the parser produces: a staged `BuildId`, a `Vec<BuildId>`, a
    /// `ParsedArguments`, …
    type Output;

    /// Parse the construct at the context's current position.
    // The output-plus-delta pair is the decided signature (DESIGN_RATIONALE.md [§dd-dr:parsers-engine]);
    // splitting it into a named type would only rename the complexity.
    #[allow(clippy::type_complexity)]
    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (Self::Output, Option<Box<ParsingStateDelta<L>>>)>;
}

/// One resolved callable invocation, as handed to
/// [`CallableSpec::make_invocation_parser`]: the dispatch loop resolves the trigger
/// token (via [`ParseDriver::resolve_command`], or the
/// resolution riding on a `Specials` token), builds this value, and moves it into the
/// invocation parser the spec's factory returns.
///
/// When the parser runs, the trigger token has already been **consumed whole** by the
/// dispatching arm (`move_to(token, TokenEdge::EndPastPostSpace)`, syntactic
/// post-space included) — see
/// [`StdInvocationParser`]'s documentation for the full contract.
///
/// The bundle is the **resolution result plus the token**: what the trigger *is*,
/// where it lies, and every other detail of it are reader questions — ask
/// `cx.tokens` (or, in [`FromInvocation::from_invocation`], the `tokens` parameter)
/// about [`token`](Invocation::token):
/// [`token_kind`](TokenReader::token_kind),
/// [`source_span_of`](TokenReader::source_span_of),
/// [`position_at`](TokenReader::position_at).
///
/// [`CallableSpec::make_invocation_parser`]: crate::spec::CallableSpec::make_invocation_parser
pub struct Invocation<'a, L: Lang> {
    /// The invocation form the trigger resolved to.
    pub callable_type: L::CallableTypeId,
    /// The trigger's invocation spelling, as written (the name the resolver matched).
    /// The *standard* parser stores an owned copy on the node; a takeover composition
    /// may store a name of its own (an environment node records the environment's
    /// name, not `begin`).
    pub name: &'a str,
    /// The behavior spec driving the parse.
    pub spec: &'a Arc<dyn CallableSpec<L>>,
    /// The token that triggered this invocation. Only a [`TokenReader`] interprets
    /// it: hand it back to `cx.tokens` to learn what the trigger is and where it lies.
    pub token: &'a Token<L>,
}

impl<L: Lang> fmt::Debug for Invocation<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Invocation")
            .field("callable_type", &self.callable_type)
            .field("name", &self.name)
            .field("spec", &self.spec)
            .field("token", &self.token)
            .finish()
    }
}

/// Opt-in constructor contract on a language's invocation-syntax payload
/// ([`Lang::InvocationSyntax`], bounded by the core
/// [`InvocationSyntax`](crate::state::InvocationSyntax) trait): build the
/// recorded trigger-spelling facts from one resolved [`Invocation`].
///
/// Consulted by the **standard staging sites** —
/// [`ParseContext::stage_invocation`] (and through it
/// [`StdInvocationParser`] and the expression-position bare-callable staging) plus
/// the preset's specials sites — under a bound-where-used
/// (`where L::InvocationSyntax: FromInvocation<L>`): a standard parser's knowledge
/// about a custom payload is exactly "what the invocation bundle shows", and the
/// bound says so. The bundle carries the token that triggered the invocation, and
/// the reader arrives alongside, so the constructor can ask what was matched
/// (spelling, escape character) and where — the syntactic post-space, say.
///
/// Deliberately **separate from the required data bound**: a language whose
/// payload cannot be built from an `Invocation` alone stages its callables through
/// custom parsers (via [`stage_node`](ParseContext::stage_node)) and never
/// implements this trait — but driving the standard engine requires it (the
/// standard dispatch loop reaches [`StdInvocationParser`] through the defaulted
/// spec factory). techy implements it for `()` (records nothing), and the
/// latexlike preset for its payload enum, so `Lang`s with `InvocationSyntax = ()`
/// and latexlike-family languages satisfy the bound out of the box.
pub trait FromInvocation<L: Lang>: Sized {
    /// The payload recording `invocation`'s trigger spelling. Pure transcription:
    /// asks `tokens` what the token that triggered the invocation is
    /// ([`token_kind`](TokenReader::token_kind)) and where any spelling it records
    /// lies, since only the reader knows either. Performs no parsing and consumes
    /// nothing; the reader arrives as a shared borrow for the duration of the call,
    /// so the implementation cannot move the stream.
    fn from_invocation(
        invocation: &Invocation<'_, L>,
        tokens: &dyn TokenReader<'_, L>,
    ) -> Self;
}

/// The no-record payload: nothing to transcribe.
impl<L: Lang> FromInvocation<L> for () {
    fn from_invocation(_invocation: &Invocation<'_, L>, _tokens: &dyn TokenReader<'_, L>) {}
}

// `pub(crate)`: the span-tiling test language below is shared with the parser test
// modules that exercise it (only in test builds — the module is `cfg(test)`).
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::source::{Source, Span};
    use crate::engine::StdParseDriver;
    use crate::error::Recovery;
    use crate::scopes::ScopeStack;
    use crate::state::{StateData, TrivialLang};
    use crate::token::{
        CommandRules, CommentRules, ForbiddenCharsRules, GroupRules, ParagraphRules,
        SpecialsRules, TokenListReader, TokenRules, WhitespaceRules,
    };
    use alloc::vec;

    /// The tiled counterpart of [`RelaxedStdLang`]: same associated types, the
    /// default `OBEYS_SPAN_TILING` (`true`).
    #[derive(Debug, Clone, Copy)]
    pub(crate) struct PlainLang;
    impl TrivialLang for PlainLang {}

    // `Features = AllLangFeatures` (via `TrivialLang`): the plain block literals
    // below only typecheck once the per-feature stores normalize to the blocks
    // themselves.
    pub(crate) fn min_rules<L: Lang<Features = crate::state::AllLangFeatures>>() -> TokenRules<L> {
        TokenRules {
            whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
            paragraphs: ParagraphRules { enabled: true },
            groups: GroupRules {
                enabled: true,
                rules: Vec::new(),
                temporary: Vec::new(),
                expecting_close: None,
            },
            commands: CommandRules { enabled: true, rules: Vec::new() },
            comments: CommentRules { enabled: true, rules: Vec::new() },
            specials: SpecialsRules { enabled: true },
            forbidden_chars: ForbiddenCharsRules { chars: "".into() },
        }
    }

    pub(crate) fn state<L>() -> Arc<ParsingState<L>>
    where
        L: Lang<Features = crate::state::AllLangFeatures, StateExt = ()>,
        L::ModeId: Default,
    {
        Arc::new(ParsingState::new(StateData {
            rules: min_rules(),
            scopes: ScopeStack::new(),
            mode: Default::default(),
            ext: (),
        }))
    }

    /// A test condition — recorded through the recovery entry point by
    /// [`RecoveringParser`].
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestCondition;

    impl fmt::Display for TestCondition {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "test condition")
        }
    }

    impl crate::error::DiagnosticInfo for TestCondition {
        const IDENTIFIER: &'static str = "test.constructs.test-condition";
    }

    /// A probing construct parser: records the input state it ran under and the
    /// enclosing-state stack depth it saw; consumes nothing, stages nothing.
    #[derive(Default)]
    struct StackProbeParser {
        seen_state: Option<Arc<ParsingState<PlainLang>>>,
        seen_depth: Option<usize>,
    }

    impl ConstructParser<PlainLang> for StackProbeParser {
        type Output = ();

        fn parse(
            &mut self,
            cx: &mut ParseContext<'_, '_, PlainLang>,
        ) -> ConstructParserResult<PlainLang, ((), Option<Box<ParsingStateDelta<PlainLang>>>)>
        {
            self.seen_state = Some(Arc::clone(&cx.state));
            self.seen_depth = Some(cx.session.state_stack().len());
            Ok(((), None))
        }
    }

    /// A construct parser that reports one [`TestCondition`] through the recovery
    /// entry point (tolerant: records and continues; strict: aborts).
    struct RecoveringParser;

    impl ConstructParser<PlainLang> for RecoveringParser {
        type Output = ();

        fn parse(
            &mut self,
            cx: &mut ParseContext<'_, '_, PlainLang>,
        ) -> ConstructParserResult<PlainLang, ((), Option<Box<ParsingStateDelta<PlainLang>>>)>
        {
            cx.recover(TestCondition, cx.here())?;
            Ok(((), None))
        }
    }

    // --- the reader-answered spans (here / source_span_within) ----------------------

    /// A context over `content`, with a real [`StdTokenReader`] behind it.
    fn with_std_context<R>(
        content: &str,
        f: impl FnOnce(&mut ParseContext<'_, '_, PlainLang>, &Arc<Source>) -> R,
    ) -> R {
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = crate::token::StdTokenReader::new(&source);
        let mut session = ParserSession::new();
        let driver = StdParseDriver::new(Recovery::Strict, ());
        let mut cx = ParseContext::new(
            &mut reader,
            state(),
            &mut session,
            &driver);
        f(&mut cx, &source)
    }

    #[test]
    fn here_is_the_empty_span_at_the_reader_position() {
        with_std_context("abc", |cx, source| {
            let at = cx.here();
            assert_eq!(at.range(), 0..0);
            assert!(Arc::ptr_eq(at.source(), source));

            // It follows the reader.
            let token = cx.tokens.peek(&cx.state).unwrap();
            cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
            assert_eq!(cx.here().range(), 1..1);
        });
    }

    #[test]
    fn source_span_within_spans_two_positions_and_lifts_an_incoherent_pair() {
        with_std_context("abc", |cx, _source| {
            let begin = cx.tokens.position_here();
            let token = cx.tokens.peek(&cx.state).unwrap();
            let end = cx.tokens.position_at(&token, TokenEdge::EndPastPostSpace);
            assert_eq!(cx.source_span_within(&begin, &end).unwrap().range(), 0..1);

            // The reversed pair delimits nothing: an implementation error, not a
            // panic and not a source condition.
            let error = cx.source_span_within(&end, &begin).unwrap_err();
            let condition = error
                .data()
                .downcast_ref::<ImplementationError>()
                .expect("an ImplementationError condition");
            assert!(
                condition.detail.contains("do not delimit one range of one source"),
                "unexpected detail: {}",
                condition.detail
            );
        });
    }

    // --- the span-tiling declaration (Lang::OBEYS_SPAN_TILING) ----------------------

    /// A language that says nothing declares span tiling: the default is `true`, and
    /// this is checked at compile time so no parse can disagree.
    const _: () = assert!(PlainLang::OBEYS_SPAN_TILING);
    const _: () = assert!(<crate::latexlike::Latexlike as Lang>::OBEYS_SPAN_TILING);

    /// A language declaring that its parse trees are **not** span-tiled, tokenized by
    /// the standard reader. What it exercises here is the declaration itself and the
    /// [`ParseContext::source_span_within`] dispatch; how the parsers record content
    /// under the declaration is a separate concern, so the parse test below asserts
    /// structure only.
    #[derive(Debug, Clone, Copy)]
    pub(crate) struct RelaxedStdLang;

    impl Lang for RelaxedStdLang {
        const OBEYS_SPAN_TILING: bool = false;

        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Tokenization = crate::token::StdTokenization;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = StdParseDriver<crate::engine::ScopesCommandResolver<RelaxedStdLang>>;

        fn make_node_ext(
            _kind: &NodeKind<Self>,
            _span: &SourceSpan<Self::SourceOrigin>,
            _state: &Arc<ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    const _: () = assert!(!RelaxedStdLang::OBEYS_SPAN_TILING);

    /// The callable type id [`RelaxedStdLang`]'s driver resolves command tokens
    /// under — an arbitrary value every test defining macros for it shares.
    pub(crate) const RELAXED_MACRO: u32 = 10;

    /// [`RelaxedStdLang`]'s driver: command tokens resolve through the state's scope
    /// stack under [`RELAXED_MACRO`], so a test can define macros for the language the
    /// way it does for a tiled one.
    pub(crate) fn relaxed_driver(
        recovery: Recovery,
    ) -> StdParseDriver<crate::engine::ScopesCommandResolver<RelaxedStdLang>> {
        StdParseDriver::new(
            recovery,
            crate::engine::ScopesCommandResolver { command_type: RELAXED_MACRO },
        )
    }

    #[test]
    fn a_language_that_does_not_obey_span_tiling_parses_over_the_standard_reader() {
        // The declaration is a fact the parsers consult, never a bound: the standard
        // reader and the standard parsers serve such a language unchanged, and the
        // tree comes out with the structure the input has.
        let mut rules = min_rules::<RelaxedStdLang>();
        rules.groups.rules = vec![Arc::new(crate::token::GroupRule {
            group_type: 0,
            open: "{".into(),
            close: "}".into(),
        })];
        let seed = Arc::new(ParsingState::new(StateData {
            rules,
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }));
        let language = crate::engine::Language::new(relaxed_driver(Recovery::Strict), seed);
        let parsed = language.parse("a{b}c").expect("the parse runs");

        let root = parsed.tree.root();
        let kinds: Vec<_> = root
            .children()
            .iter()
            .map(|child| match child.kind() {
                NodeKind::Chars { .. } => "chars",
                NodeKind::Group(_) => "group",
                other => panic!("unexpected node kind: {other:?}"),
            })
            .collect();
        assert_eq!(kinds, ["chars", "group", "chars"]);
        // The all-trees law holds for such a tree, exactly as for a tiled one.
        crate::node::validate_tree(&parsed.tree).expect("the all-trees law holds");
    }

    #[test]
    fn source_span_within_describes_the_stretch_when_the_language_does_not_obey_tiling() {
        let source: Arc<Source> = Arc::new(Source::new("abc"));
        let mut reader = crate::token::StdTokenReader::new(&source);
        let mut session = ParserSession::new();
        let driver = relaxed_driver(Recovery::Strict);
        let cx = ParseContext::new(
            &mut reader,
            state::<RelaxedStdLang>(),
            &mut session,
            &driver);

        let begin = cx.tokens.position_here();
        let token = cx.tokens.peek(&cx.state).unwrap();
        let end = cx.tokens.position_at(&token, TokenEdge::EndPastPostSpace);

        // An ordered pair still spans exactly what it delimits …
        assert_eq!(cx.source_span_within(&begin, &end).unwrap().range(), 0..1);
        // … and the reversed pair is no longer an error: the reader's described span
        // is recorded, with no assumption made about it (here, the empty span at
        // `begin`, which is what `StdTokenReader` describes for an inverted pair).
        let described = cx
            .source_span_within(&end, &begin)
            .expect("no error under OBEYS_SPAN_TILING = false");
        assert_eq!(described.range(), 1..1);
    }

    // --- stage_invocation's three end cases -----------------------------------------

    /// A spec with no arguments and no behavior: `stage_invocation` reads only its
    /// identity off the bundle.
    #[derive(Debug)]
    struct TestSpec;

    impl crate::serialize::SerializableObject<PlainLang> for TestSpec {}
    impl CallableSpec<PlainLang> for TestSpec {}

    /// Stage a callable over the token of `content` at index `trigger` as the
    /// trigger, with the given children and end, and report the staged node's span
    /// range.
    fn stage_invocation_span(
        content: &str,
        trigger: usize,
        end: EndUnderTest,
        children: impl FnOnce(&mut ParseContext<'_, '_, PlainLang>) -> Vec<BuildId>,
    ) -> Result<core::ops::Range<usize>, ParseError> {
        with_std_context(content, |cx, _source| {
            let spec: Arc<dyn CallableSpec<PlainLang>> = Arc::new(TestSpec);
            for _ in 0..trigger {
                let skipped = cx.tokens.peek(&cx.state).unwrap();
                cx.tokens.move_to(&skipped, TokenEdge::EndPastPostSpace);
            }
            let token = cx.tokens.peek(&cx.state).unwrap();
            // The dispatch contract: the trigger is consumed before the parser runs.
            cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
            let children = children(cx);
            let invocation = Invocation {
                callable_type: 0u32,
                name: "t",
                spec: &spec,
                token: &token,
            };
            let end = match end {
                EndUnderTest::Standard => None,
                EndUnderTest::Here => Some(cx.tokens.position_here()),
                EndUnderTest::BeforeTheTrigger => {
                    Some(cx.tokens.position_at(&token, TokenEdge::StartBeforePreSpace))
                }
            };
            // A callable's children must be tiled by its regions: one content slot
            // covers whatever the fixture staged.
            let slots = match children.len() as u32 {
                0 => ParsedSlots::empty(),
                count => ParsedSlots::new(alloc::vec![crate::node::ParsedSlot::new(
                    crate::node::ChildRegion::new(
                        0..count,
                        crate::node::ContentNodes::InRegion(0..count),
                    ),
                    "content",
                    crate::node::SlotRole::Content,
                    (),
                )]),
            };
            let id = cx.stage_invocation(
                &invocation,
                ParsedArguments::empty(),
                slots,
                children,
                end.as_ref(),
            )?;
            Ok(cx.staged_nodes().get(id).unwrap().span().range())
        })
    }

    enum EndUnderTest {
        /// `None` — the standard rule.
        Standard,
        /// `Some` at the reader's current position.
        Here,
        /// `Some` before the trigger's own start (a contract violation).
        BeforeTheTrigger,
    }

    /// Stage one chars node over `range` as a child.
    fn chars_child(
        range: core::ops::Range<usize>,
    ) -> impl FnOnce(&mut ParseContext<'_, '_, PlainLang>) -> Vec<BuildId> {
        move |cx| {
            // The source the reader is serving, as the reader reports it.
            let here = cx.here();
            let span = SourceSpan::new(here.source(), range.clone());
            let id = cx
                .stage_node(
                    NodeKind::chars(span.span()),
                    span,
                    Arc::clone(&cx.state),
                    Vec::new(),
                )
                .unwrap();
            alloc::vec![id]
        }
    }

    /// Stage one chars node over `range` of a **second** source as a child — the
    /// standard rule's "last child in another source" branch.
    fn foreign_child(
        range: core::ops::Range<usize>,
    ) -> impl FnOnce(&mut ParseContext<'_, '_, PlainLang>) -> Vec<BuildId> {
        move |cx| {
            let other: Arc<Source> = Arc::new(Source::new("elsewhere"));
            let span = SourceSpan::new(&other, range.clone());
            let id = cx
                .stage_node(
                    NodeKind::chars(span.span()),
                    span,
                    Arc::clone(&cx.state),
                    Vec::new(),
                )
                .unwrap();
            alloc::vec![id]
        }
    }

    #[test]
    fn stage_invocation_ends_at_the_explicit_position() {
        // The reader stands past the trigger's post-space, and two more characters
        // are consumed before staging: the explicit end claims them.
        let range = stage_invocation_span("abcd", 0, EndUnderTest::Here, |cx| {
            for _ in 0..2 {
                let token = cx.tokens.peek(&cx.state).unwrap();
                cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
            }
            Vec::new()
        })
        .unwrap();
        assert_eq!(range, 0..3);
    }

    #[test]
    fn stage_invocation_ends_at_the_last_child_under_the_standard_rule() {
        let range = stage_invocation_span("abcd", 0, EndUnderTest::Standard, chars_child(1..3))
            .unwrap();
        assert_eq!(range, 0..3);
    }

    #[test]
    fn stage_invocation_ends_at_the_reader_for_a_childless_shape() {
        // No child: the node ends where the reader stands — just past the trigger.
        let range =
            stage_invocation_span("abcd", 0, EndUnderTest::Standard, |_cx| Vec::new()).unwrap();
        assert_eq!(range, 0..1);
    }

    #[test]
    fn stage_invocation_ignores_a_last_child_staged_in_another_source() {
        // The child's span cannot end this node — it is in another source — so the
        // node ends where the reader stands (just past the trigger), exactly as for
        // a childless shape.
        let range =
            stage_invocation_span("abcd", 0, EndUnderTest::Standard, foreign_child(3..9))
                .unwrap();
        assert_eq!(range, 0..1);
    }

    #[test]
    fn stage_invocation_reports_an_end_before_the_trigger_as_an_implementation_error() {
        // (a) an explicit end that precedes the trigger's start (the trigger's own
        // pre-space edge).
        let error =
            stage_invocation_span(" abcd", 0, EndUnderTest::BeforeTheTrigger, |_cx| Vec::new())
                .unwrap_err();
        assert_invalid_node_span(&error);

        // (b) the standard rule with a last child that ends before the trigger
        // starts (the trigger is the third character here, the child the first).
        let error =
            stage_invocation_span("abcd", 2, EndUnderTest::Standard, chars_child(0..1))
                .unwrap_err();
        assert_invalid_node_span(&error);
    }

    fn assert_invalid_node_span(error: &ParseError) {
        let condition = error
            .data()
            .downcast_ref::<ImplementationError>()
            .expect("an ImplementationError condition");
        assert!(
            condition.detail.contains("invalid node span"),
            "unexpected detail: {}",
            condition.detail
        );
    }

    // --- the descent entry point (parse_construct) --------------------------------

    #[test]
    fn parse_construct_state_none_matches_explicit_current_state_clone() {
        let source: Arc<Source> = Arc::new(Source::new("xy"));
        let st = state();
        let mut reader: TokenListReader<'_, PlainLang> = TokenListReader::new(&source, vec![]);
        let mut session = ParserSession::new();
        let driver = StdParseDriver::new(Recovery::Tolerant, ());
        let mut cx = ParseContext::new(
            &mut reader,
            Arc::clone(&st),
            &mut session,
            &driver);

        // `state: None`: the sub-parse runs under the current state, scoped (one
        // enclosing-state stack entry), and the outer state is restored after.
        let mut probe = StackProbeParser::default();
        cx.parse_construct(&mut probe, None, None).unwrap();
        assert!(Arc::ptr_eq(probe.seen_state.as_ref().unwrap(), &st));
        let depth_under_none = probe.seen_depth.unwrap();
        assert_eq!(depth_under_none, 1);
        assert!(Arc::ptr_eq(&cx.state, &st), "outer state restored");
        assert_eq!(cx.session.state_stack().len(), 0, "stack entry popped");

        // `Some(Arc::clone(&cx.state))`: observably identical — same input state,
        // same enclosing-state stack depth seen inside, same restore.
        let mut probe = StackProbeParser::default();
        let explicit = Arc::clone(&cx.state);
        cx.parse_construct(&mut probe, Some(explicit), None).unwrap();
        assert!(Arc::ptr_eq(probe.seen_state.as_ref().unwrap(), &st));
        assert_eq!(probe.seen_depth.unwrap(), depth_under_none);
        assert!(Arc::ptr_eq(&cx.state, &st), "outer state restored");
        assert_eq!(cx.session.state_stack().len(), 0, "stack entry popped");
    }

    #[test]
    fn parse_construct_frame_reaches_the_diagnostic_snapshot() {
        let source: Arc<Source> = Arc::new(Source::new("xy"));
        let st = state();
        let mut reader: TokenListReader<'_, PlainLang> = TokenListReader::new(&source, vec![]);
        let mut session = ParserSession::new();
        let driver = StdParseDriver::new(Recovery::Tolerant, ());
        let mut cx =
            ParseContext::new(&mut reader, st, &mut session, &driver);

        let frame = Frame {
            title: FrameTitle::Static("construct frame"),
            span: SourceSpan::new(&source, Span::new(0, 1)),
        };
        cx.parse_construct(&mut RecoveringParser, None, Some(frame)).unwrap();
        assert!(cx.session.snapshot_frames().is_empty(), "frame popped on return");

        let diagnostic = session.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.frames().len(), 1);
        assert_eq!(diagnostic.frames()[0].title(), "construct frame");
        assert_eq!(diagnostic.frames()[0].span().range(), 0..1);
    }

    #[test]
    fn parse_construct_pops_the_frame_on_the_err_path() {
        let source: Arc<Source> = Arc::new(Source::new("xy"));
        let st = state();
        let mut reader: TokenListReader<'_, PlainLang> = TokenListReader::new(&source, vec![]);
        let mut session = ParserSession::new();
        let driver = StdParseDriver::new(Recovery::Strict, ());
        let mut cx =
            ParseContext::new(&mut reader, st, &mut session, &driver);

        let frame = Frame {
            title: FrameTitle::Static("construct frame"),
            span: SourceSpan::new(&source, Span::new(0, 1)),
        };
        let err =
            cx.parse_construct(&mut RecoveringParser, None, Some(frame)).unwrap_err();
        assert!(
            cx.session.snapshot_frames().is_empty(),
            "frame popped on the Err path"
        );
        // The strict abort snapshotted the frame while it was live.
        assert_eq!(err.frames().len(), 1);
        assert_eq!(err.frames()[0].title(), "construct frame");
    }

    /// The `&dyn Fn` hot-path callbacks ([`TokenStopKind::Predicate`],
    /// [`StopSpec::node`], both [`ChildStateSpec`] `Compute` arms) materialize
    /// their `Result` on every consultation — per token or per descent — because
    /// the callee is opaque to the optimizer (unlike the statically dispatched
    /// `Lang`/driver hooks, where an infallible monomorphized body folds the
    /// channel away). This pins the measured cost: the `Result` wrapper adds
    /// zero bytes over the bare [`ParseError`] arm (the `Ok` payload fits the
    /// error type's niches) and the whole `Result` stays within one cache line
    /// ([`ConstructParserResult`] carries the identical [`ParseError`] arm), so
    /// the `Err` arm is deliberately **not** boxed — re-measure here before
    /// restructuring.
    #[test]
    fn hot_path_result_payloads_stay_within_one_cache_line() {
        use core::mem::size_of;
        type Origin = Option<String>;

        let parse_error = size_of::<ParseError<Origin>>();
        // `TokenStopKind::Predicate` / `StopSpec::node` (pre-sweep: `bool`, 1 B).
        let predicate = size_of::<Result<bool, ParseError<Origin>>>();
        // The two `Compute` arms (pre-sweep: `Arc<ParsingState<_>>`, 8 B).
        let compute =
            size_of::<Result<Arc<ParsingState<PlainLang>>, ParseError<Origin>>>();

        // The niche fit: wrapping either `Ok` payload in `Result` costs nothing
        // over the bare error arm.
        assert_eq!(
            predicate, parse_error,
            "Predicate/StopSpec::node Result outgrew the bare ParseError"
        );
        assert_eq!(
            compute, parse_error,
            "Compute-arm Result outgrew the bare ParseError"
        );
        assert!(parse_error <= 64, "ParseError outgrew one cache line: {parse_error} B");
    }
}
