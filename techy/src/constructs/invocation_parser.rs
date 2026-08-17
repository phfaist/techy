//! [`StdInvocationParser`]: the default declarative invocation parser, returned by
//! [`CallableSpec::make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser)
//! (pylatexenc's `LatexMacroCallParser`-family, behind one factory).
//! The full invocation-parsing contract — what every parser returned by that factory
//! runs under — lives on [`StdInvocationParser`]'s own documentation.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::engine::{Frame, FrameTitle};
use crate::node::{
    BuildId, ChildRegion, ParsedArgument, ParsedArguments, ParsedSlots,
};
use crate::source::SourceSpan;
use crate::spec::{CallableSpec, FrameRole};
use crate::token::TokenEdge;
use crate::state::{Lang, ParsingStateDelta};

use super::{
    ConstructParser, ConstructParserResult, FromInvocation, Invocation, ParseContext,
};

/// Parse a callable's declared arguments at the reader's position — the argument half of
/// [`StdInvocationParser`], shared with environment-shaped compositions.
///
/// Iterates `callable_spec.arguments()` in invocation order, running each argument's
/// parser under the argument's own state — the spec's `parsing_state_delta` stacked on
/// `cx.state`, session-mediated, reverted structurally — and
/// collects the provided regions' nodes into one child list plus one [`ParsedArgument`]
/// entry per spec (absent arguments keep their entry and contribute no nodes). The
/// returned regions are staged in child-list offsets, ready for the caller's
/// [`ParsedArguments`] record.
///
/// Each argument runs under its traceback frame (`argument #N of ‘\frac’`):
/// `name_span` is the invocation spelling's span, quoted into the title at snapshot
/// time — the spec is de-keyed, so the spelling must come from the caller (an
/// environment composition passes its *name*'s span, so frames quote `align`, not
/// `\begin`).
///
/// Public as a takeover-composition building block:
/// an environment-shaped `make_invocation_parser` override runs this same loop between
/// reading its `\begin{name}` name group and parsing its body.
pub fn parse_declared_arguments<L: Lang>(
    cx: &mut ParseContext<'_, '_, L>,
    callable_spec: &Arc<dyn CallableSpec<L>>,
    name: &SourceSpan<L::SourceOrigin>,
) -> ConstructParserResult<L, (Vec<BuildId>, Vec<ParsedArgument<L>>)> {
    let argument_specs = callable_spec.arguments();
    let mut children: Vec<BuildId> = Vec::new();
    let mut arguments: Vec<ParsedArgument<L>> = Vec::with_capacity(argument_specs.len());
    for (index, argument_spec) in argument_specs.iter().enumerate() {
        let argument_state = match &argument_spec.parsing_state_delta {
            Some(delta) => cx.derive_state(delta)?,
            None => Arc::clone(&cx.state),
        };
        // The argument's traceback frame, anchored where the argument's region starts.
        let frame = Frame {
            title: FrameTitle::Callable {
                spec: Arc::clone(callable_spec),
                role: FrameRole::Argument { index },
                name: name.clone(),
            },
            span: cx.here(),
        };
        let result = cx.with_frame(frame, |cx| {
            cx.with_parsing_state(argument_state, |cx| {
                argument_spec.parser.parse_argument(cx, argument_spec)
            })
        });
        match result? {
            Some(region) => {
                let start = children.len() as u32;
                children.extend_from_slice(&region.nodes);
                let end = children.len() as u32;
                // The parser's minted ext travels into the record ([`ParsedArgument`]'s
                // population-is-initialization contract); an absent argument gets none.
                arguments.push(ParsedArgument::provided(
                    Arc::clone(argument_spec),
                    ChildRegion::new(start..end, region.content),
                    region.ext,
                ));
            }
            None => arguments.push(ParsedArgument::absent(Arc::clone(argument_spec))),
        }
    }
    Ok((children, arguments))
}

/// The standard declarative invocation parser — the default returned by
/// [`CallableSpec::make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser)
/// — and the carrier of the invocation-parsing contract, which any takeover
/// parser supplied by an overridden factory runs under too. A tier-2 temporary
/// (see [`core::constructs`](crate::core::constructs)), constructed per
/// invocation by the spec's factory.
///
/// # Contract
///
/// Constructed around the resolved [`Invocation`], which travels inside the parser
/// instance. The **caller consumes the
/// trigger token whole** — `move_to_edge(token, TokenEdge::EndPastPostSpace)`,
/// syntactic post-space included —
/// before running the parser (the dispatch-loop arm that peeked it, mirroring the
/// [`GroupParser`](super::GroupParser) contract; loop progress holds by construction,
/// since no invocation parser can forget to consume its trigger). The token's pre-space
/// is likewise the caller's (housed as sibling content). A takeover parser that needs
/// the trigger's post-space bytes raw (the `\verb` idiom) repositions the reader
/// itself, at the trigger's own [`End`](crate::token::TokenEdge) edge —
/// `move_to_edge(token, TokenEdge::End)`, where the token proper ends and its
/// post-space begins.
///
/// `cx.state` is the invocation's **base** state: the caller resolves any
/// [`InvocationChildState`](super::InvocationChildState) policy first and scopes the
/// state structurally (swap/revert).
///
/// # Arguments
///
/// The parser iterates the spec's [`ArgumentSpec`](crate::spec::ArgumentSpec)s in
/// invocation order, running each argument's
/// [`ArgumentParser`](crate::spec::ArgumentParser) under the argument's
/// own state — the spec's `parsing_state_delta` stacked on the invocation's base
/// (session-mediated, so the transition is observed), reverted structurally after; the
/// argument's whole extent, noise scan included, runs under it. Each provided argument
/// contributes its region's nodes to the child list and a staged
/// [`ChildRegion`](crate::node::ChildRegion) to the [`ParsedArguments`] record; an
/// absent argument keeps its entry (spec included — the record is self-describing) and
/// contributes nothing. Missing-mandatory recovery is the argument parser's own
/// detection-site business: by the time `parse_argument` reports absent, any
/// diagnostic is already recorded.
///
/// The node's span runs from the trigger token through the last child (the children
/// block is span-contiguous by construction: each region starts where the previous
/// ended). Argument parsers return no after-effect deltas (an argument scopes no state
/// beyond its own extent) and neither does this parser.
///
/// # Invocation syntax
///
/// [`CallableData::invocation_syntax`](crate::node::CallableData::invocation_syntax)
/// records the language's trigger-spelling
/// facts, minted from the [`Invocation`] via the standard constructor
/// ([`FromInvocation`](super::FromInvocation)) inside
/// [`stage_invocation`](ParseContext::stage_invocation). The latexlike payload
/// records e.g. **exactly the trigger token's syntactic post-space** — the
/// name-terminating whitespace the tokenizer already claimed as invocation syntax
/// (pylatexenc's `macro_post_space`); nothing beyond it is ever claimed:
/// whitespace after a single-character command (`\& b`) or after a final argument
/// is ordinary sibling/region content, exactly as TeX treats it. With arguments
/// present that recorded post-space sits **between** the name and the first
/// argument region — a sub-range of the node's span, no longer necessarily
/// trailing (whitespace invariant 3).
///
/// # Slots
///
/// `StdInvocationParser` is macro-shaped: it parses no body and records empty
/// [`ParsedSlots`]. Slots are record-level vocabulary with no spec-side declaration
/// (there is nothing a spec could declare that this
/// parser wouldn't parse): body content is inseparable from terminator syntax and from
/// invocation facts like the `\end{name}` back-reference, so a body-bearing spec
/// overrides
/// [`make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser) with a
/// composition that drives [`EnvironmentBodyParser`](super::EnvironmentBodyParser) and
/// mints its own [`ParsedSlot`](crate::node::ParsedSlot) records (the
/// argument half is shared as [`parse_declared_arguments`]) — and says "I take
/// material" via [`requires_content`](crate::spec::CallableSpec::requires_content), the
/// expression-position guard's channel.
pub struct StdInvocationParser<'a, 's, L: Lang> {
    invocation: Invocation<'a, 's, L>,
}

impl<'a, 's, L: Lang> StdInvocationParser<'a, 's, L> {
    /// A parser for the given resolved invocation (the default body of
    /// [`CallableSpec::make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser)).
    pub fn new(invocation: Invocation<'a, 's, L>) -> StdInvocationParser<'a, 's, L> {
        StdInvocationParser { invocation }
    }
}

impl<L: Lang> ConstructParser<L> for StdInvocationParser<'_, '_, L>
where
    L::InvocationSyntax: FromInvocation<L>,
{
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (BuildId, Option<Box<ParsingStateDelta<L>>>)> {
        let token = self.invocation.token;
        // The invocation spelling (trigger minus syntactic post-space) titles the
        // argument frames.
        let name = cx.tokens.source_span_between(token, TokenEdge::Start, TokenEdge::End);
        let (children, arguments) =
            parse_declared_arguments(cx, self.invocation.spec, &name)?;

        // The transcription-case staging shorthand: callable_type/name/spec and the
        // invocation-syntax payload transcribed from the bundle; `None` = the std
        // span rule — trigger through the last staged child, the trigger's span
        // alone for argument-less shapes (6.4 parity).
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

impl<L: Lang> fmt::Debug for StdInvocationParser<'_, '_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StdInvocationParser")
            .field("invocation", &self.invocation)
            .finish()
    }
}
