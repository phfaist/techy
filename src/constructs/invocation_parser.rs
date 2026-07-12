//! [`StdInvocationParser`]: the default declarative invocation parser, returned by
//! [`CallableSpec::make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser)
//! (pylatexenc's `LatexMacroCallParser`-family, behind one factory).
//!
//! # Contract
//!
//! Constructed around the resolved [`Invocation`], which travels inside the parser
//! instance (decided July 2026, DESIGN_RATIONALE.md §3.6). The **caller consumes the
//! trigger token whole** — `move_past(token, true)`, syntactic post-space included —
//! before running the parser (the dispatch-loop arm that peeked it, mirroring the
//! [`GroupParser`](super::GroupParser) contract; loop progress holds by construction,
//! since no invocation parser can forget to consume its trigger). The token's pre-space
//! is likewise the caller's (housed as sibling content). A takeover parser that needs
//! the trigger's post-space bytes raw (the `\verb` idiom) repositions the reader itself
//! via `move_past(invocation.token, false)`.
//!
//! `cx.state` is the invocation's **base** state: the caller resolves any
//! [`InvocationChildState`](super::InvocationChildState) policy first and scopes the
//! state structurally (swap/revert).
//!
//! # Arguments (Phase 6.5)
//!
//! The parser iterates the spec's [`ArgumentSpec`](crate::spec::ArgumentSpec)s in
//! invocation order, running each argument's [`ArgumentParser`] under the argument's
//! own state — the spec's `parsing_state_delta` stacked on the invocation's base
//! (session-mediated, so the transition is observed), reverted structurally after; the
//! argument's whole extent, noise scan included, runs under it. Each provided argument
//! contributes its region's nodes to the child list and a staged
//! [`ChildRegion`](crate::node::ChildRegion) to the [`ParsedArguments`] record; an
//! absent argument keeps its entry (spec included — the record is self-describing) and
//! contributes nothing. Missing-mandatory recovery is the argument parser's own
//! detection-site business (§3.8): by the time `parse_argument` reports absent, any
//! diagnostic is already recorded.
//!
//! The node's span runs from the trigger token through the last child (the children
//! block is span-contiguous by construction: each region starts where the previous
//! ended). Argument parsers return no after-effect deltas (an argument scopes no state
//! beyond its own extent) and neither does this parser.
//!
//! # Post-space (amended July 2026, Phase 6.4, user decision)
//!
//! [`CallableData::post_space`] records **exactly the trigger token's syntactic
//! post-space** — the name-terminating whitespace the tokenizer already claimed as
//! invocation syntax (pylatexenc's `macro_post_space`). Nothing beyond it is ever
//! claimed: whitespace after a single-character command (`\& b`) or after a final
//! argument is ordinary sibling/region content, exactly as TeX treats it. With
//! arguments present the recorded post-space thus sits **between** the name and the
//! first argument region — a sub-range of the node's span, no longer necessarily
//! trailing (§3.5 invariant 3 as amended).
//!
//! # Slots
//!
//! `StdInvocationParser` is macro-shaped: it parses no slots and debug-asserts that the
//! spec declares none. Slot content is inseparable from terminator syntax, which is
//! parser business, not spec data (§3.6) — an environment-shaped spec overrides
//! [`make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser) with a
//! composition that drives [`EnvironmentBodyParser`](super::EnvironmentBodyParser)
//! (Phase 6.6; the argument half is shared as [`parse_declared_arguments`]).

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::mem;

use crate::node::{
    BuildId, CallableData, ChildRegion, NodeKind, ParsedArgument, ParsedArguments,
    ParsedSlots,
};
use crate::source::{SourceSpan, TextContent};
use crate::spec::ArgumentSpec;
use crate::state::{Lang, ParsingStateDelta};

use super::{ConstructParser, ConstructParserResult, Invocation, ParseContext};

/// Parse a callable's declared arguments at the reader's position — the argument half of
/// [`StdInvocationParser`], shared with environment-shaped compositions (Phase 6.6).
///
/// Iterates `argument_specs` in invocation order, running each argument's parser under
/// the argument's own state — the spec's `parsing_state_delta` stacked on `cx.state`
/// (§3.6 decided semantics 2), session-mediated, reverted structurally — and collects
/// the provided regions' nodes into one child list plus one [`ParsedArgument`] entry per
/// spec (absent arguments keep their entry and contribute no nodes). The returned
/// regions are staged in child-list offsets, ready for the caller's
/// [`ParsedArguments`] record.
pub(crate) fn parse_declared_arguments<L: Lang>(
    cx: &mut ParseContext<'_, '_, L>,
    argument_specs: &[Arc<ArgumentSpec<L>>],
) -> ConstructParserResult<L, (Vec<BuildId>, Vec<ParsedArgument<L>>)> {
    let mut children: Vec<BuildId> = Vec::new();
    let mut arguments: Vec<ParsedArgument<L>> = Vec::new();
    for argument_spec in argument_specs {
        let argument_state = match &argument_spec.parsing_state_delta {
            Some(delta) => cx.session.derived_state(&cx.state, delta),
            None => Arc::clone(&cx.state),
        };
        let outer_state = mem::replace(&mut cx.state, argument_state);
        let result = argument_spec.parser.parse_argument(cx, argument_spec);
        cx.state = outer_state;
        match result? {
            Some(region) => {
                let start = children.len() as u32;
                children.extend_from_slice(&region.nodes);
                let end = children.len() as u32;
                arguments.push(ParsedArgument::provided(
                    Arc::clone(argument_spec),
                    ChildRegion::new(start..end, region.content),
                ));
            }
            None => arguments.push(ParsedArgument::absent(Arc::clone(argument_spec))),
        }
    }
    Ok((children, arguments))
}

/// The standard declarative invocation parser: a tier-2 temporary constructed per
/// invocation by the spec's factory (see the module docs for the contract).
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

impl<L: Lang> ConstructParser<L> for StdInvocationParser<'_, '_, L> {
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (BuildId, Option<ParsingStateDelta<L>>)> {
        assert!(
            // ### PhF - FIXME: I turned this from debug_assert!() into an assert!(), but this should really be
            // a special type of error that is returned.  We want someting like ParseError::MisconfiguredLang
            // or ParseError::InternalError or something like that for this.
            self.invocation.spec.slots().is_empty(),
            "StdInvocationParser is macro-shaped: a spec declaring slots overrides \
             make_invocation_parser with a composition that knows its terminator syntax \
             (e.g., EnvironmentBodyParser)"
        );

        let token = self.invocation.token;
        let (children, arguments) =
            parse_declared_arguments(cx, self.invocation.spec.arguments())?;

        // Span: trigger through the last child (regions are span-contiguous); the
        // trigger's span alone for argument-less shapes (6.4 parity).
        let end = match children.last() {
            Some(last) => {
                let staged = cx.session.builder.staged_nodes();
                staged.get(*last).expect("the child was just staged").span().end()
            }
            None => token.span.end,
        };

        let data = CallableData {
            callable_type: self.invocation.callable_type,
            name: self.invocation.name.into(),
            spec: Arc::clone(self.invocation.spec),
            arguments: ParsedArguments::from(arguments),
            slots: ParsedSlots::empty(),
            // Exactly the trigger token's syntactic post-space (module docs).
            post_space: TextContent::Spanned(token.post_space()),
            ext: Default::default(),
        };
        let id = cx.session.builder.add(
            NodeKind::callable(data),
            SourceSpan::new(&cx.source, token.span.start..end),
            Arc::clone(&cx.state),
            children,
        );
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
