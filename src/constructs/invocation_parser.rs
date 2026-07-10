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
//! # Post-space (amended July 2026, Phase 6.4, user decision)
//!
//! [`CallableData::post_space`] records **exactly the trigger token's syntactic
//! post-space** — the name-terminating whitespace the tokenizer already claimed as
//! invocation syntax (pylatexenc's `macro_post_space`). Nothing beyond it is ever
//! claimed: whitespace after a single-character command (`\& b`) or after a final
//! argument is ordinary sibling/region content, exactly as TeX treats it. This
//! supersedes the earlier "invocation parser claims following whitespace" rule (the
//! `claim_post_space` helper of §3.5 invariant 3 was never shipped).
//!
//! # Subphase scope
//!
//! Phase 6.4 covers zero-argument, zero-slot callables: the name (owned copy) from the
//! invocation, empty [`ParsedArguments`]/[`ParsedSlots`], the node's span = the trigger
//! token's span. Argument parsing ([`ArgumentSpec`](crate::spec::ArgumentSpec) iteration)
//! lands in 6.5, slots (environment bodies) in 6.6.

use alloc::sync::Arc;
use alloc::vec;
use core::fmt;

use crate::node::{BuildId, CallableData, NodeKind, ParsedArguments, ParsedSlots};
use crate::source::{SourceSpan, TextContent};
use crate::state::{Lang, ParsingStateDelta};

use super::{ConstructParser, ConstructParserResult, Invocation, ParseContext};

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
        // Phase 6.4: zero-argument, zero-slot callables only (6.5 iterates
        // spec.arguments(), 6.6 adds slots).
        debug_assert!(
            self.invocation.spec.arguments().is_empty()
                && self.invocation.spec.slots().is_empty(),
            "StdInvocationParser parses declared arguments/slots from Phase 6.5/6.6 on"
        );

        let token = self.invocation.token;
        let data = CallableData {
            callable_type: self.invocation.callable_type,
            name: self.invocation.name.into(),
            spec: Arc::clone(self.invocation.spec),
            arguments: ParsedArguments::empty(),
            slots: ParsedSlots::empty(),
            // Exactly the trigger token's syntactic post-space (module docs); a trailing
            // sub-range of the token's span, hence of the node's.
            post_space: TextContent::Spanned(token.post_space()),
            ext: Default::default(),
        };
        let id = cx.session.builder.add(
            NodeKind::callable(data),
            SourceSpan::new(&cx.source, token.span.range()),
            Arc::clone(&cx.state),
            vec![],
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
