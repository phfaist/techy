//! [`ParseDriver`]: the Lang-provided parse-behavior object (Phase 7.2,
//! DESIGN_RATIONALE.md §3.6).
//!
//! A driver is the **instance** face of a language's parse-time behavior: while
//! [`Lang`](crate::state::Lang) stays the compile-time bundle of hooks belonging to
//! layers callable outside a driven parse (state transitions, tokenizer specials, node
//! finalization), everything that only runs *while a parse is driven* lives here — as
//! `&self` methods on a value, so behavior can carry configuration that static `Lang`
//! hooks never could (a recovery policy, a preset's package registry).
//!
//! The driver owns four concerns:
//!
//! - **policy** — the [`Recovery`] knob ([`recovery`](ParseDriver::recovery)) and the
//!   funnels consulting it ([`recover`](ParseDriver::recover),
//!   [`probe_token`](ParseDriver::probe_token));
//! - **parse-time hooks** (migrated off `Lang`, July 2026) —
//!   [`resolve_command`](ParseDriver::resolve_command),
//!   [`make_paragraph_break_node`](ParseDriver::make_paragraph_break_node),
//!   [`refine_diagnostic`](ParseDriver::refine_diagnostic),
//!   [`observe_transition`](ParseDriver::observe_transition);
//! - **the group descent-delta channel** —
//!   [`group_interior_delta`](ParseDriver::group_interior_delta), the data plug that
//!   lets a group class change the parsing state of its interior (a math group entering
//!   math mode is one line: a delta with a [`mode`](crate::state::ParsingStateDelta::mode)
//!   override);
//! - **construct provision** — [`make_nodes_parser`](ParseDriver::make_nodes_parser),
//!   [`make_group_parser`](ParseDriver::make_group_parser),
//!   [`make_invocation_parser`](ParseDriver::make_invocation_parser). Every descent
//!   site routes through the [`ParseContext`](crate::constructs::ParseContext) wrappers
//!   ([`parse_nodes`](crate::constructs::ParseContext::parse_nodes)/[`parse_group`](crate::constructs::ParseContext::parse_group)),
//!   so one override applies uniformly to the whole parse.
//!
//! The driver is bound into the bundle as [`Lang::Driver`](crate::state::Lang::Driver)
//! and reaches parsers as [`ParseContext::driver`](crate::constructs::ParseContext::driver) — **concretely typed through `L`**,
//! so a preset's parsers call preset helper methods (inherent methods on the driver
//! type) with zero downcasts, while generic code sees only this trait.
//!
//! Drivers are shared and immutable (`&self`, `Send + Sync`): per-parse mutable state
//! belongs to the [`ParserSession`] (whose derivation memos the driver-consulting
//! helpers [`ParserSession::derived_state`]/[`ParserSession::group_interior_state`]
//! own), and per-language *data* belongs to the parsing state. [`StdParseDriver`] is
//! the all-defaults implementation — a plain `Recovery` carrier, and the
//! [`SimpleLang`](crate::state::SimpleLang) default.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;

use crate::constructs::{
    ChildStateSpec, ConstructParser, ConstructParserResult, GroupParser, Invocation,
    NodesOutcome, NodesParser, StopSpec,
};
use crate::error::{DiagnosticData, ParseError, Recovery};
use crate::node::{BuildId, NodeKind};
use crate::source::{Source, SourceSpan, Span};
use crate::spec::CallableSpec;
use crate::state::{Lang, ParsingState, ParsingStateDelta};
use crate::token::{GroupRule, Token, TokenReader};

use super::ParserSession;

/// The Lang-provided parse-behavior object (see the module docs): policy, migrated
/// parse-time hooks, the group descent-delta channel, and construct provision — all
/// defaulted, so `impl ParseDriver<MyLang> for MyDriver {}` is a complete driver.
///
/// Implementations are **stateless behavior objects**: `&self` everywhere, shared
/// across parses (`Send + Sync`), carrying configuration but never per-parse state —
/// that lives on the [`ParserSession`]. Hooks consulted by the session's memoized
/// derivation helpers ([`group_interior_delta`](ParseDriver::group_interior_delta))
/// must be pure; the per-method docs state their contracts.
pub trait ParseDriver<L: Lang>: fmt::Debug + Send + Sync {
    // --- policy -----------------------------------------------------------------

    /// The tolerant-parsing policy this driver drives under. The default is
    /// [`Recovery::Strict`]; [`StdParseDriver`] carries the knob as a field.
    ///
    /// Consulted by the default [`recover`](ParseDriver::recover) and
    /// [`probe_token`](ParseDriver::probe_token) paths — a custom policy beyond the
    /// strict/tolerant enum overrides those methods instead.
    fn recovery(&self) -> Recovery {
        Recovery::Strict
    }

    /// Detection-site recovery — **the recover funnel** (DESIGN_RATIONALE.md §3.8),
    /// reached through [`ParseContext::recover`](crate::constructs::ParseContext::recover): applies
    /// [`refine_diagnostic`](ParseDriver::refine_diagnostic) exactly once, then
    /// records the condition as an error-severity diagnostic and returns `Ok(())`
    /// (tolerant — the caller continues with its site's local recovery) or returns it
    /// as a [`ParseError`] to bubble (strict — nobody continues past an `Err`).
    ///
    /// Overriding this method replaces the *policy*, not the plumbing: richer policies
    /// (per-condition severities, whitelists, diagnostic budgets) decide per call
    /// between [`ParserSession::recover`]'s record-or-abort modes. An override takes on
    /// the refinement responsibility — route condition data through
    /// [`refine_diagnostic`](ParseDriver::refine_diagnostic) before recording, or
    /// document that refinement does not apply.
    fn recover(
        &self,
        session: &mut ParserSession<L>,
        state: &ParsingState<L>,
        data: Box<dyn DiagnosticData>,
        span: SourceSpan<L::SourceOrigin>,
    ) -> Result<(), ParseError<L::SourceOrigin>> {
        let data = self.refine_diagnostic(data, state);
        session.recover(self.recovery(), data, span)
    }

    /// Probe the token at the reader's position under `state`, mapping a tokenizer
    /// error per the recovery policy — the **probing peek** of the argument-probe
    /// protocol, reached through [`ParseContext::probe_token`](crate::constructs::ParseContext::probe_token): strict mode aborts
    /// with the token error (mirroring the content loop); tolerant mode reports `None`
    /// **without diagnosing or consuming** — the caller treats the position as
    /// unusable (argument absent, terminator malformed) and the enclosing content loop
    /// re-reads the error and applies its own token recovery, avoiding a double
    /// report.
    ///
    /// A token error carrying **no** recovery is unrecoverable and aborts even under
    /// [`Recovery::Tolerant`] — mirroring the content loop, whose re-read would abort
    /// anyway; reporting `None` first would only add a spurious absent-position
    /// recovery (and its diagnostic) on the way down.
    fn probe_token<'s>(
        &self,
        tokens: &mut dyn TokenReader<'s, L>,
        source: &Arc<Source<L::SourceOrigin>>,
        session: &ParserSession<L>,
        state: &Arc<ParsingState<L>>,
    ) -> ConstructParserResult<L, Option<Token<'s, L>>> {
        match tokens.peek(state) {
            Ok(token) => Ok(Some(token)),
            Err(error) => {
                if self.recovery() == Recovery::Tolerant && error.recovery().is_some() {
                    return Ok(None);
                }
                let span = SourceSpan::new(source, error.span());
                Err(ParseError::from_token_error(error.kind().clone(), span)
                    .with_frames(session.snapshot_frames()))
            }
        }
    }

    // --- parse-time hooks (migrated off `Lang`, July 2026) -----------------------

    /// Resolve a [`Command`](crate::token::TokenKind::Command) token to its invocation
    /// form and behavior spec. Typically implemented by a preset dispatching to the
    /// state's libraries via a [`CallableQuery`](crate::scopes::CallableQuery) — the
    /// token carries the fired escape character for syntax disambiguation. `Specials`
    /// tokens need no hook: recognition = resolution, the token already carries its
    /// spec (that asymmetry is decided — specials resolution is token-time and stays
    /// on [`Lang::scan_specials`](crate::state::Lang::scan_specials); command
    /// resolution is parse-time and lives here).
    ///
    /// An implementation returns [`Resolved`](CommandResolution::Resolved) to dispatch
    /// the invocation, or [`Unresolved`](CommandResolution::Unresolved) — the parse
    /// loops then diagnose the command as unresolvable and recover (span-backed
    /// chars-node fallback, DESIGN_RATIONALE.md §3.8). The failure's optional `detail`
    /// string is surfaced on that diagnostic: the place for a resolver to say *why*
    /// ("searched libraries x, y, z"; "load the {amsmath} library for this command").
    ///
    /// The default resolves nothing, with a detail reporting that command resolution
    /// is not implemented. A missing implementation has no compile-time signal — a
    /// language that enables commands but never overrides this hook would otherwise
    /// see every command fail with a bare "cannot resolve", nothing pointing at the
    /// actual cause.
    fn resolve_command(
        &self,
        state: &ParsingState<L>,
        token: &Token<'_, L>,
    ) -> CommandResolution<L> {
        let _ = (state, token);
        CommandResolution::Unresolved {
            detail: Some(
                "command resolution is not implemented by this language’s driver — \
                 implement ‘ParseDriver::resolve_command’ or use a preset"
                    .into(),
            ),
        }
    }

    /// The node kind representing a paragraph break. The *core* stages the returned
    /// kind with the token's span and the current state (a driver cannot stage nodes
    /// itself); a preset may return a callable-shaped kind (FLM's paragraph
    /// constructs) without any core change.
    ///
    /// **Constraint:** the kind is staged with *no children*, so a callable-shaped
    /// kind must carry no argument regions and no slots — the builder's region-tiling
    /// assert panics otherwise. (Structurally intrinsic: this hook has no
    /// session/builder and cannot stage children.)
    ///
    /// The default preserves the whitespace-as-chars invariant (§3.5): a
    /// whitespace-only `Chars` kind, span-backed over the full token span (newlines
    /// included).
    fn make_paragraph_break_node(
        &self,
        state: &ParsingState<L>,
        token: &Token<'_, L>,
    ) -> NodeKind<L> {
        let _ = state;
        NodeKind::chars(token.span)
    }

    /// Condition refinement (DESIGN_RATIONALE.md §3.8): replace a condition payload
    /// with a language-specific one before it is recorded. Applied exactly once, in
    /// the default [`recover`](ParseDriver::recover) path — at the driver level, where
    /// the parsing state is in scope. The default is the identity.
    ///
    /// An implementation downcasts `data`, decides from the state, and returns either
    /// the original box or its own [`DiagnosticInfo`](crate::error::DiagnosticInfo)
    /// type — e.g. FLM mapping a forbidden-`$` token condition to a
    /// `DollarMathDisabled` whose `Display` explains the configuration option. The
    /// replacement is *structured*: tools see (and can attach quickfixes to) the
    /// refined condition, not just better prose. State-dependent information the
    /// message needs is baked into the refined payload's fields here — conditions stay
    /// self-contained after the parse (no state references inside errors, no lazy
    /// rendering).
    fn refine_diagnostic(
        &self,
        data: Box<dyn DiagnosticData>,
        state: &ParsingState<L>,
    ) -> Box<dyn DiagnosticData> {
        let _ = state;
        data
    }

    /// Per-transition **observation** (DESIGN_RATIONALE.md §3.6): called by the
    /// session-mediated derivation helpers ([`ParserSession::derived_state`],
    /// [`ParserSession::group_interior_state`]) on **every** transition event — memo
    /// hits included, which is what
    /// [`Lang::finalize_transition`](crate::state::Lang::finalize_transition)
    /// structurally cannot see (it runs once per unique *derivation*, not once per
    /// transition). Parse-history accumulation ("how many times did the parse enter
    /// math mode") belongs here, in the session's
    /// [`SessionExt`](crate::state::Lang::SessionExt) — never in
    /// `finalize_transition`, where structural scope reverts and memoization would
    /// make counts wrong twice over.
    ///
    /// Observational only: it receives the already-frozen `new` state and cannot alter
    /// the transition's outcome (the session layer is data-equivalent to
    /// [`ParsingState::derived`]). The default does nothing.
    fn observe_transition(
        &self,
        ext: &mut L::SessionExt,
        prev: &ParsingState<L>,
        new: &ParsingState<L>,
        delta: &ParsingStateDelta<L>,
    ) {
        let _ = (ext, prev, new, delta);
    }

    // --- the group descent-delta channel ------------------------------------------

    /// The extra state delta a group descent applies to its interior, keyed on the
    /// entered rule — the data plug for "entering this group class changes the state"
    /// (the latexlike math plug: a math-class rule returns
    /// `ParsingStateDelta::new().mode(MathInline)`, and
    /// [`Lang::finalize_transition`](crate::state::Lang::finalize_transition)
    /// interprets the mode change). `None` — the default — means the canonical descent
    /// derivation alone.
    ///
    /// **Must be a deterministic pure function of `(base, rule)`** — the result is
    /// memoized per `(base, rule)` by [`ParserSession::group_interior_state`] (`Arc`
    /// identities; the hook runs on memo **miss** only, so a call-count-dependent
    /// implementation would be observably wrong). The returned delta is merged with
    /// the descent invariant: the interior's
    /// [`expecting_group_close`](crate::token::TokenRules::expecting_group_close) is
    /// always the entered rule — a returned override of that field is discarded.
    fn group_interior_delta(
        &self,
        base: &ParsingState<L>,
        rule: &Arc<GroupRule<L>>,
    ) -> Option<ParsingStateDelta<L>> {
        let _ = (base, rule);
        None
    }

    // --- construct provision -------------------------------------------------------

    /// The factory producing the content-loop parser for one nodes descent (group
    /// interiors, environment bodies, the top-level drive) — a fresh boxed parser per
    /// descent, ownership moved to the caller. Reached through
    /// [`ParseContext::parse_nodes`](crate::constructs::ParseContext::parse_nodes), which every descent site routes through, so an
    /// override applies uniformly (the supported seam for a custom dispatch loop).
    ///
    /// The default is the standard [`NodesParser`] over the given stop conditions and
    /// descent-state policies. A custom parser must uphold the `NodesParser` output
    /// contract its callers rely on (a [`NodesOutcome`] whose staged nodes tile the
    /// consumed extent; no pass-through delta).
    fn make_nodes_parser<'p>(
        &'p self,
        stop: StopSpec<'p, L>,
        child_states: ChildStateSpec<'p, L>,
    ) -> Box<dyn ConstructParser<L, Output = NodesOutcome<L>> + 'p> {
        Box::new(NodesParser::new(stop).with_child_states(child_states))
    }

    /// The factory producing the parser for one group descent (the consumed
    /// `GroupOpen` token's facts: open span and resolved rule) — a fresh boxed parser
    /// per descent. Reached through [`ParseContext::parse_group`](crate::constructs::ParseContext::parse_group) at every group
    /// descent site.
    ///
    /// The default is the standard [`GroupParser`], which derives the interior state
    /// through [`ParseContext::group_interior_state`](crate::constructs::ParseContext::group_interior_state) (where
    /// [`group_interior_delta`](ParseDriver::group_interior_delta) merges in) — prefer
    /// the delta channel for state-shaped customization; override this factory only
    /// for structurally different group parses.
    fn make_group_parser<'p>(
        &'p self,
        open_span: Span,
        rule: Arc<GroupRule<L>>,
        child_states: ChildStateSpec<'p, L>,
    ) -> Box<dyn ConstructParser<L, Output = BuildId> + 'p> {
        Box::new(GroupParser::new(open_span, rule).with_child_states(child_states))
    }

    /// Interception seam over
    /// [`CallableSpec::make_invocation_parser`]: the dispatch loops obtain every
    /// invocation parser through the driver, and the default delegates to the resolved
    /// spec's own factory — specs keep owning their invocation behavior; the driver
    /// merely gets a uniform veto/wrap point (instrumentation, per-language parser
    /// substitution) that no per-spec override could provide.
    ///
    /// The caller has already consumed the trigger token whole; see the
    /// [`StdInvocationParser`](crate::constructs::StdInvocationParser) module docs for
    /// the invocation-parser contract an implementation must uphold.
    fn make_invocation_parser<'a, 's>(
        &'a self,
        invocation: Invocation<'a, 's, L>,
    ) -> Box<dyn ConstructParser<L, Output = BuildId> + 'a>
    where
        's: 'a,
    {
        let spec = invocation.spec;
        spec.make_invocation_parser(invocation)
    }
}

/// The all-defaults [`ParseDriver`]: a plain [`Recovery`] carrier, implementing the
/// trait for **every** language — the [`SimpleLang`](crate::state::SimpleLang) default
/// driver, and the strict-parsing default value.
///
/// ```
/// # use techy::{Recovery, StdParseDriver};
/// let strict = StdParseDriver::default();
/// assert_eq!(strict.recovery, Recovery::Strict);
/// let tolerant = StdParseDriver { recovery: Recovery::Tolerant };
/// # let _ = tolerant;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StdParseDriver {
    /// The tolerant-parsing policy to drive under (default: [`Recovery::Strict`]).
    pub recovery: Recovery,
}

impl StdParseDriver {
    /// A driver with the given recovery policy.
    pub fn new(recovery: Recovery) -> StdParseDriver {
        StdParseDriver { recovery }
    }
}

impl Default for StdParseDriver {
    fn default() -> Self {
        StdParseDriver { recovery: Recovery::Strict }
    }
}

impl<L: Lang> ParseDriver<L> for StdParseDriver {
    fn recovery(&self) -> Recovery {
        self.recovery
    }
}

/// A successful command resolution (the payload of [`CommandResolution::Resolved`]):
/// which invocation form the command resolved to, and the behavior spec to drive its
/// parse — exactly what the dispatch loop needs to build an
/// [`Invocation`](crate::constructs::Invocation) (the core cannot know a preset's type
/// ids).
pub struct ResolvedCallable<L: Lang> {
    /// The invocation form (latexlike: macro / environment / …).
    pub callable_type: L::CallableTypeId,
    /// The resolved behavior spec.
    pub spec: Arc<dyn CallableSpec<L>>,
}

// Manual impls: derives would demand `L:` bounds although only associated types (already
// bounded) and an `Arc` are stored.

impl<L: Lang> Clone for ResolvedCallable<L> {
    fn clone(&self) -> Self {
        ResolvedCallable { callable_type: self.callable_type, spec: Arc::clone(&self.spec) }
    }
}

impl<L: Lang> fmt::Debug for ResolvedCallable<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedCallable")
            .field("callable_type", &self.callable_type)
            .field("spec", &self.spec)
            .finish()
    }
}

/// The result of [`ParseDriver::resolve_command`]: a resolution to dispatch, or an
/// [`Unresolved`](CommandResolution::Unresolved) failure whose optional `detail` says
/// *why* — surfaced verbatim on the unresolvable-command diagnostic. Any resolution
/// layer may fill it in: the trait's default hook reports that command resolution is
/// not implemented at all; a library-backed resolver might report where it searched
/// ("searched libraries x, y, z") or hint at a fix ("load the {amsmath} library for
/// this command").
#[non_exhaustive]
pub enum CommandResolution<L: Lang> {
    /// The command resolved: dispatch this invocation.
    Resolved(ResolvedCallable<L>),
    /// The command did not resolve; the parse loops diagnose it as unresolvable and
    /// recover (span-backed chars fallback, DESIGN_RATIONALE.md §3.8).
    Unresolved {
        /// Optional human-facing detail on why resolution failed, appended to the
        /// diagnostic's message and serialized with the condition. `None` when there
        /// is nothing to say beyond "the name did not resolve".
        detail: Option<String>,
    },
}

/// `Some`/`None` from a lookup maps to `Resolved`/`Unresolved` with no detail — the
/// bridge for resolvers built on `Option`-returning queries (library lookups).
impl<L: Lang> From<Option<ResolvedCallable<L>>> for CommandResolution<L> {
    fn from(resolved: Option<ResolvedCallable<L>>) -> Self {
        match resolved {
            Some(resolved) => CommandResolution::Resolved(resolved),
            None => CommandResolution::Unresolved { detail: None },
        }
    }
}

impl<L: Lang> Clone for CommandResolution<L> {
    fn clone(&self) -> Self {
        match self {
            CommandResolution::Resolved(resolved) => {
                CommandResolution::Resolved(resolved.clone())
            }
            CommandResolution::Unresolved { detail } => {
                CommandResolution::Unresolved { detail: detail.clone() }
            }
        }
    }
}

impl<L: Lang> fmt::Debug for CommandResolution<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandResolution::Resolved(resolved) => {
                f.debug_tuple("Resolved").field(resolved).finish()
            }
            CommandResolution::Unresolved { detail } => {
                f.debug_struct("Unresolved").field("detail", detail).finish()
            }
        }
    }
}
