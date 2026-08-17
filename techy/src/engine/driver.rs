//! [`ParseDriver`]: the Lang-provided parse-behavior object.
//!
//! A driver is the **instance** face of a language's parse-time behavior: while
//! [`Lang`](crate::state::Lang) stays the compile-time bundle of hooks belonging to
//! layers callable outside a driven parse (state transitions, tokenizer specials, node
//! finalization), everything that only runs *while a parse is driven* lives here — as
//! `&self` methods on a value, so behavior can carry configuration that static `Lang`
//! hooks never could (a recovery policy, a preset's package registry).
//!
//! The driver owns five concerns:
//!
//! - **policy** — the [`Recovery`] knob ([`recovery`](ParseDriver::recovery)) and the
//!   funnels consulting it ([`recover`](ParseDriver::recover),
//!   [`probe_token`](ParseDriver::probe_token));
//! - **parse-time hooks** —
//!   [`resolve_command`](ParseDriver::resolve_command),
//!   [`make_paragraph_break_node`](ParseDriver::make_paragraph_break_node),
//!   [`refine_diagnostic`](ParseDriver::refine_diagnostic),
//!   [`observe_transition`](ParseDriver::observe_transition);
//! - **source resolution** — the
//!   [`source_resolver`](ParseDriver::source_resolver) accessor exposing the
//!   embedding environment's [`SourceResolver`] for `\input`-like external
//!   references (`None` = resolves nothing);
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
//! the one canned implementation — the recovery knob, a pluggable [`CommandResolver`]
//! strategy, an optional source resolver — and the
//! [`TrivialLang`](crate::state::TrivialLang) default.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::constructs::{
    ChildStateSpec, ConstructParser, ConstructParserResult, FromInvocation, GroupParser,
    Invocation, NodesOutcome, NodesParser, StopSpec,
};
use crate::error::{DiagnosticData, Diagnostics, ParseError, Recovery};
use crate::node::{BuildId, NodeKind};
use crate::scopes::{CallableQuery, CallableSyntax};
use crate::source::{
    IntoSourceResolver, Source, SourceOrigin, SourceResolver, SourceSpan,
};
use crate::spec::CallableSpec;
use crate::state::{Lang, ParsingState, ParsingStateDelta, ParsingStateStack};
use crate::token::{
    GroupRule, StdStreamPosition, StdToken, StdTokenReader, TokenKind, TokenReader,
};


use super::ParserSession;

/// The Lang-provided parse-behavior object, grouping five concerns: the recovery
/// policy, the parse-time hooks (command resolution, paragraph-break emission,
/// diagnostic refinement, transition observation, event lowering), source
/// resolution, the group descent-delta channel, and construct provision. Every
/// method but [`make_token_reader`](ParseDriver::make_token_reader) is defaulted,
/// and that one's standard body is the one-liner
/// `Box::new(StdTokenReader::new(source))`, so
/// `impl ParseDriver<MyLang> for MyDriver { /* make_token_reader */ }` is a complete
/// driver.
/// (Parsing depth is limited by the engine-owned
/// [`StdDescentGuard`](super::StdDescentGuard), not a driver
/// concern; its configuration travels on the [`Language`](super::Language) value,
/// [`with_descent_guard_init`](super::Language::with_descent_guard_init).)
///
/// Implementations are **stateless behavior objects**: `&self` everywhere, shared
/// across parses (`Send + Sync`), carrying configuration but never per-parse state —
/// that lives on the [`ParserSession`]. Hooks consulted by the session's memoized
/// derivation helpers ([`group_interior_delta`](ParseDriver::group_interior_delta))
/// must be pure; the per-method docs state their contracts.
///
/// # Wrapping a driver
///
/// Several defaulted methods call sibling methods **through `self`**, and the
/// per-method docs name each such call: the default
/// [`recover`](ParseDriver::recover) calls
/// [`refine_diagnostic`](ParseDriver::refine_diagnostic) and
/// [`recovery`](ParseDriver::recovery); the default
/// [`probe_token`](ParseDriver::probe_token) reads
/// [`recovery`](ParseDriver::recovery); the default
/// [`make_invocation_parser`](ParseDriver::make_invocation_parser) delegates to
/// the resolved spec's own factory. This matters for a **delegating driver** — a
/// type that wraps another driver and forwards methods to it: a forwarded method
/// re-dispatches through the *inner* driver's `self`, so forwarding `recover`
/// while overriding `refine_diagnostic` on the wrapper means the wrapper's
/// override silently never runs. A wrapper therefore forwards every method it
/// does not override — the defaulted ones included, so a hook the inner driver
/// overrides is not silently replaced by this trait's default — and
/// re-implements, rather than forwards, any defaulted method whose body calls a
/// method the wrapper overrides.
pub trait ParseDriver<L: Lang>: fmt::Debug + Send + Sync {
    // --- policy -----------------------------------------------------------------

    /// The tolerant-parsing policy this driver drives under. The default is
    /// [`Recovery::Strict`]; [`StdParseDriver`] carries the policy as a field.
    ///
    /// Consulted by the default [`recover`](ParseDriver::recover) and
    /// [`probe_token`](ParseDriver::probe_token) paths — a custom policy beyond the
    /// strict/tolerant enum overrides those methods instead.
    ///
    /// Deliberately infallible: this is a pure read of configured policy, with no
    /// computation that could fail. Embedding or binding code whose implementation
    /// can still fail should report the failure through the embedding's own channel
    /// and answer [`Recovery::Strict`] — the conservative policy: a strict parse
    /// aborts on problems instead of continuing under a policy nobody chose.
    fn recovery(&self) -> Recovery {
        Recovery::Strict
    }

    /// The retention cap for the parse's diagnostics sink, consulted once per
    /// parse: [`Language::parse_source`](super::Language::parse_source) seeds the
    /// session's [`Diagnostics`] with [`Diagnostics::with_limit`] when this
    /// answers `Some(limit)`; on `None` — the default — the sink keeps the
    /// standard cap ([`Diagnostics::DEFAULT_LIMIT`]). Code driving construct
    /// parsers over a hand-built [`ParserSession`] applies the cap itself (the
    /// session's [`diagnostics`](ParserSession::diagnostics) field is public).
    fn diagnostics_limit(&self) -> Option<usize> {
        None
    }

    /// Detection-site recovery — **the recovery hook**, reached through the parsers'
    /// recovery entry point
    /// ([`ParseContext::recover`](crate::constructs::ParseContext::recover)). The
    /// default calls [`refine_diagnostic`](ParseDriver::refine_diagnostic) and
    /// [`recovery`](ParseDriver::recovery) **through `self`** (a delegating driver
    /// must account for that — see *Wrapping a driver* on the trait): it applies
    /// the refinement exactly once, then — per the policy `recovery()` answers —
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

    // --- tokenization ---------------------------------------------------------

    /// Build the token reader for one parse over `source` — **the one place custom
    /// tokenization is installed**.
    ///
    /// Both reader-construction sites go through this hook:
    /// [`Language::parse_source`](super::Language::parse_source) for the root parse and
    /// [`ParseContext::parse_attached_source`](crate::constructs::ParseContext::parse_attached_source)
    /// for an attached (included) source. A driver that returns its own reader thereby
    /// tokenizes the whole parse its way, while the *types* involved stay fixed by the
    /// language ([`Lang::StreamPosition`](crate::state::Lang::StreamPosition)).
    ///
    /// The standard body — right for every language tokenized by
    /// [`StdTokenReader`](crate::token::StdTokenReader), which is every language whose
    /// `StreamPosition` is [`StdStreamPosition`](crate::token::StdStreamPosition):
    ///
    /// ```ignore
    /// fn make_token_reader<'s>(
    ///     &'s self,
    ///     source: &'s Arc<Source<L::SourceOrigin>>,
    /// ) -> Box<dyn TokenReader<'s, L> + 's> {
    ///     Box::new(StdTokenReader::new(source))
    /// }
    /// ```
    ///
    /// This method has no default: the default body would have to name a concrete
    /// reader type, and no single reader type serves a language whose stream positions
    /// are its own.
    ///
    /// The returned reader borrows both `self` and `source` for the parse's extent, so
    /// a driver may hand it configuration it holds.
    fn make_token_reader<'s>(
        &'s self,
        source: &'s Arc<Source<L::SourceOrigin>>,
    ) -> Box<dyn TokenReader<'s, L> + 's>;

    /// Probe the token at the reader's position under `state`, mapping a tokenizer
    /// error per the recovery policy (the default reads it through
    /// `self.`[`recovery()`](ParseDriver::recovery)) — the **probing peek** of the argument-probe
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
        session: &ParserSession<L>,
        state: &Arc<ParsingState<L>>,
    ) -> ConstructParserResult<L, Option<L::Token>> {
        match tokens.peek(state) {
            Ok(token) => Ok(Some(token)),
            Err(error) => {
                if self.recovery() == Recovery::Tolerant && error.recovery().is_some() {
                    return Ok(None);
                }
                // The token error already says where it is, in its own reader's source.
                let span = error.span().clone();
                Err(ParseError::from_token_error(error.kind().clone(), span)
                    .with_frames(session.snapshot_frames()))
            }
        }
    }

    // --- parse-time hooks (migrated off `Lang`, July 2026) -----------------------

    /// Resolve a [`Command`](crate::token::TokenKind::Command) token to its
    /// invocation form and behavior spec. Typically implemented by a preset
    /// dispatching to the state's libraries via a
    /// [`CallableQuery`](crate::scopes::CallableQuery) — carrying the name and the
    /// fired escape character for syntax disambiguation. `Specials` tokens need no
    /// hook: recognition = resolution, the token already carries its spec (that
    /// asymmetry is deliberate — specials resolution is token-time and stays on
    /// [`Lang::scan_specials`](crate::state::Lang::scan_specials); command resolution
    /// is parse-time and lives here).
    ///
    /// The hook receives the triggering **token** and a shared, call-scoped reference
    /// to the **reader that produced it**, so a language may take over resolution by
    /// inspecting any detail of the token it needs:
    /// [`tokens.token_kind(token)`](crate::token::TokenReader::token_kind) for what it
    /// is, [`source_span_of`](crate::token::TokenReader::source_span_of) or
    /// [`position_at`](crate::token::TokenReader::position_at) for where it is. The
    /// reference is shared, so the resolver cannot move the stream. Anything other
    /// than a `Command` token is a caller-contract violation; answer
    /// [`Unresolved`](CommandResolution::Unresolved).
    ///
    /// An implementation returns [`Resolved`](CommandResolution::Resolved) to dispatch
    /// the invocation, or [`Unresolved`](CommandResolution::Unresolved) — the parse
    /// loops then diagnose the command as unresolvable and recover (span-backed
    /// chars-node fallback). The failure's optional `detail`
    /// string is surfaced on that diagnostic: the place for a resolver to say *why*
    /// ("searched libraries x, y, z"; "load the {amsmath} library for this command").
    ///
    /// The default resolves nothing — it delegates to the no-op [`CommandResolver`]
    /// `()`, whose detail reports that command resolution is not implemented. A
    /// missing implementation has no compile-time signal — a language that enables
    /// commands but never overrides this hook would otherwise see every command fail
    /// with a bare "cannot resolve", nothing pointing at the actual cause.
    /// ([`StdParseDriver`] does not use this default: it routes the hook through its
    /// pluggable [`CommandResolver`] strategy.)
    ///
    /// # Errors
    ///
    /// `Err` **aborts the parse** — reserve it for failures no resolution value can
    /// express, and keep the two failure channels distinct:
    ///
    /// - [`Failed`](CommandResolution::Failed) (an `Ok` value) is the **recoverable**
    ///   channel: the resolver failed operationally at this document position, the
    ///   parse diagnoses it
    ///   ([`CommandResolutionFailed`](crate::constructs::CommandResolutionFailed))
    ///   and continues with the span-backed chars recovery.
    /// - `Err` is the **abort** channel: carry
    ///   [`HookFailed`](crate::error::HookFailed) for an operational failure the
    ///   parse must not continue past (a resolver backend that is down for the whole
    ///   parse, a runtime failure in an embedding),
    ///   [`ImplementationError`](crate::constructs::ImplementationError) for a
    ///   violated library contract, or a document condition only for a diagnosis
    ///   made deliberately (aborting is strict-mode behavior; recoverable document
    ///   problems belong on the `Ok` channels).
    ///
    /// An infallible implementation wraps its resolution in `Ok(...)` and that is
    /// the only change.
    fn resolve_command(
        &self,
        state: &ParsingState<L>,
        token: &L::Token,
        tokens: &dyn TokenReader<'_, L>,
    ) -> Result<CommandResolution<L>, ParseError<L::SourceOrigin>> {
        CommandResolver::resolve_command(&(), state, token, tokens)
    }

    /// The node kind representing a paragraph break. The *core* stages the returned
    /// kind with the token's span and the current state (a driver cannot stage nodes
    /// itself); a preset may return a callable-shaped kind (FLM's paragraph
    /// constructs) without any core change.
    ///
    /// `break_span` is the paragraph-break token's span, as the reader answers it:
    /// its range ([`span`](crate::source::SourceSpan::span)) is what a span-backed
    /// kind records, and its text ([`content`](crate::source::SourceSpan::content))
    /// is what a callable-shaped kind records as the break's actual spelling
    /// (name-as-written, owned node data).
    ///
    /// **Constraint:** the kind is staged with *no children*, so a callable-shaped
    /// kind must carry no argument regions and no slots — the builder's region-tiling
    /// check rejects any such kind, and the staging site aborts with an
    /// [`ImplementationError`](crate::constructs::ImplementationError).
    /// (Structurally intrinsic: this hook has no
    /// session/builder and cannot stage children.)
    ///
    /// The default preserves the whitespace-as-chars invariant: a
    /// whitespace-only `Chars` kind, span-backed over the full token span (newlines
    /// included).
    ///
    /// Deliberately infallible: a fixed default node is always answerable — the
    /// whitespace-as-chars default requires no computation that could fail.
    /// Embedding or binding code whose implementation can still fail should report
    /// the failure through the embedding's own channel and answer the default node
    /// (`NodeKind::chars(break_span.span())`).
    fn make_paragraph_break_node(
        &self,
        state: &ParsingState<L>,
        break_span: &SourceSpan<L::SourceOrigin>,
    ) -> NodeKind<L> {
        let _ = state;
        NodeKind::chars(break_span.span())
    }

    /// Condition refinement: replace a condition payload
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
    /// message needs is stored in the refined payload's fields here — conditions stay
    /// self-contained after the parse (no state references inside errors, no lazy
    /// rendering).
    ///
    /// Deliberately infallible: the identity fallback is always sound — a refiner
    /// that cannot refine returns the payload it received. Embedding or binding
    /// code whose implementation can still fail should report the failure through
    /// the embedding's own channel and pass `data` through unchanged; the condition
    /// is then recorded unrefined.
    fn refine_diagnostic(
        &self,
        data: Box<dyn DiagnosticData>,
        state: &ParsingState<L>,
    ) -> Box<dyn DiagnosticData> {
        let _ = state;
        data
    }

    /// Per-transition **observation**: called by the
    /// session-mediated derivation helpers ([`ParserSession::derived_state`],
    /// [`ParserSession::group_interior_state`]) on **every** transition event — memo
    /// hits included, which is what
    /// [`Lang::finalize_transition`](crate::state::Lang::finalize_transition)
    /// structurally cannot see (it runs once per unique *derivation*, not once per
    /// transition). Parse-history accumulation ("how many times did the parse enter
    /// math mode") belongs here, in the session's
    /// [`SessionExt`](crate::state::Lang::SessionExt) — never in
    /// `finalize_transition`, where structural scope reverts and memoization would
    /// make counts wrong twice over. The accumulated value is handed out on
    /// [`ParseResult::session_ext`](super::ParseResult::session_ext) when the
    /// session freezes ([`ParserSession::finish`]).
    ///
    /// Observational only: it receives the already-frozen `new` state and cannot alter
    /// the transition's outcome (the session layer is data-equivalent to
    /// [`ParsingState::derived`]). The default does nothing.
    ///
    /// # The two reporting channels
    ///
    /// The `diagnostics` sink and the `Err` return play different roles — keep
    /// them apart:
    ///
    /// - **`diagnostics` records document-level observations without affecting
    ///   the parse.** An observer that diagnoses something about the document
    ///   ("this transition pattern is deprecated here") pushes a
    ///   [`Diagnostic`](crate::error::Diagnostic) of whatever severity fits and
    ///   returns `Ok(())` — an error-severity entry does **not** abort the parse,
    ///   and deciding record-versus-abort for source conditions is
    ///   [`recover`](ParseDriver::recover)'s business, not this hook's. (The sink
    ///   is the same channel
    ///   [`observe_parse_start`](ParseDriver::observe_parse_start) receives.)
    /// - **`Err` aborts the parse**, under any recovery policy — reserve it for a
    ///   truly problematic state the parse must not continue past (an observer
    ///   backend that is gone, a violated invariant in the embedding). Carry
    ///   [`HookFailed`](crate::error::HookFailed) for an operational failure,
    ///   [`ImplementationError`](crate::constructs::ImplementationError) for a
    ///   violated library contract; the derivation seam attaches the live
    ///   traceback when the error carries no frames of its own.
    ///
    /// An infallible implementation returns `Ok(())` and that is the only change.
    fn observe_transition(
        &self,
        ext: &mut L::SessionExt,
        diagnostics: &mut Diagnostics<L::SourceOrigin>,
        prev: &ParsingState<L>,
        new: &ParsingState<L>,
        delta: &ParsingStateDelta<L>,
    ) -> Result<(), ParseError<L::SourceOrigin>> {
        let _ = (ext, diagnostics, prev, new, delta);
        Ok(())
    }

    /// Once-per-parse **initialization observation**: called by
    /// [`Language::parse_source`](crate::engine::Language::parse_source) after the
    /// session is created and before any token is read — the layering-correct
    /// moment for registration-sanity diagnostics (the sink is live, and the
    /// seed's [`TokenRules`](crate::token::TokenRules) — escape characters
    /// included — are known, which no registration-time layer can see). May record
    /// **warnings/notes** into `diagnostics`; it cannot alter the parse. The
    /// default does nothing.
    ///
    /// The latexlike driver delegates to
    /// [`LatexlikeLang::check_parse_start`](crate::latexlike::LatexlikeLang::check_parse_start),
    /// which for the shipped preset runs the all-escape-shadowed provider check
    /// ([`check_provider_commands_shadowed_by_escape`](crate::scopes::check_provider_commands_shadowed_by_escape)).
    ///
    /// Attached-source sub-parses
    /// ([`parse_attached_source`](crate::constructs::ParseContext::parse_attached_source))
    /// deliberately do **not** re-fire this hook: it observes *parse
    /// initialization* (the seeded providers), not every descent.
    fn observe_parse_start(
        &self,
        source: &Arc<Source<L::SourceOrigin>>,
        seed: &Arc<ParsingState<L>>,
        diagnostics: &mut Diagnostics<L::SourceOrigin>,
    ) {
        let _ = (source, seed, diagnostics);
    }

    /// Lower one **context-dependent** transition event to an ordinary state-delta
    /// patch, given the session's live enclosing-state stack — the driver half of
    /// the two-class event contract ([`Lang::Event`]), consulted by
    /// [`ParseContext::derive_state`](crate::constructs::ParseContext::derive_state)
    /// once per event before the delta reaches the derivation point
    /// ([`ParsingState::derived`](crate::state::ParsingState::derived)).
    ///
    /// - Return `Some(patch)` to **lower** the event: the patch is merged into the
    ///   delta and the event is removed — it never reaches
    ///   [`Lang::finalize_transition`]. This is where context-dependent semantics
    ///   live (the latexlike exit-math restore scans `stack` for the innermost
    ///   non-math state and patches its token rules — minus that context's
    ///   in-flight transients — and mode back in —
    ///   [`exit_math_context_delta`](crate::latexlike::exit_math_context_delta)).
    /// - Return `None` — the default — for a **context-free** event: it stays on
    ///   the delta for [`Lang::finalize_transition`] to consume as usual.
    ///
    /// `stack` iterates innermost-first, current state first
    /// ([`ParsingStateStack`]). A returned patch should carry only overrides
    /// (rules/mode/ext/scope ops); events inside a patch are **not** lowered again
    /// — they pass through to `finalize_transition` like any context-free event.
    ///
    /// # Errors
    ///
    /// `Err` **aborts the parse** (an event carrying document-derived data can be
    /// malformed by the language's own rules — silently answering `Ok(None)` for a
    /// recognized-but-unusable event would drop it without a trace). Carry
    /// [`HookFailed`](crate::error::HookFailed) for an operational failure in the
    /// lowering code itself,
    /// [`ImplementationError`](crate::constructs::ImplementationError) for a
    /// violated library contract, or a document condition for a document diagnosis
    /// made deliberately (aborting is strict-mode behavior — there is no recovery
    /// channel at this seam). An infallible implementation wraps its answer in
    /// `Ok(...)` and that is the only change.
    fn resolve_state_event(
        &self,
        event: &L::Event,
        stack: &ParsingStateStack<L>,
    ) -> Result<Option<ParsingStateDelta<L>>, ParseError<L::SourceOrigin>> {
        let _ = (event, stack);
        Ok(None)
    }

    // --- source resolution ---------------------------------------------------------

    /// The driver's [`SourceResolver`] for `\input`-like external source references,
    /// if one is configured. The default is `None`: **this language resolves
    /// nothing** — no lookup, no I/O; an `\input`-style construct then diagnoses
    /// every use instead of resolving it.
    ///
    /// The resolver is an *embedding-environment* capability (where content comes
    /// from varies per deployment), which is why it is reached through this
    /// type-erased accessor rather than a generic driver parameter — see the
    /// asymmetry note on [`StdParseDriver`]. Shipped drivers store an
    /// `Option<Arc<dyn SourceResolver<…>>>` field set via their chainable
    /// `with_source_resolver(…)` builder; a custom driver returns whatever resolver
    /// composition it owns. Resolution itself stays *outside* the parse machinery:
    /// callers compose accessor → [`resolve_source_reference`](crate::source::resolve_source_reference)
    /// → parse, so caching frameworks can substitute either half.
    ///
    /// Deliberately infallible: this accessor only hands out an already-configured
    /// resolver — failure belongs on [`SourceResolver::resolve`], which is already
    /// fallible and reports per reference. Embedding or binding code that cannot
    /// produce its resolver should report the failure through the embedding's own
    /// channel and answer `None` (the parse then resolves nothing).
    fn source_resolver(&self) -> Option<&dyn SourceResolver<L::SourceOrigin>> {
        None
    }

    // --- the group descent-delta channel ------------------------------------------

    /// The extra state delta a group descent applies to its interior, keyed on the
    /// entered rule — the data channel for "entering this group class changes the state"
    /// (the latexlike math-interior delta: a math-class rule returns
    /// `ParsingStateDelta::new().mode(Mode::Math)`, so the interior parses in math
    /// mode). `None` — the default — means the canonical descent derivation alone.
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
    /// override applies uniformly (the supported extension point for a custom
    /// dispatch loop).
    ///
    /// The default is the standard [`NodesParser`] over the given stop conditions and
    /// descent-state policies. A custom parser must uphold the `NodesParser` output
    /// contract its callers rely on (a [`NodesOutcome`] whose staged nodes tile the
    /// consumed extent; no pass-through delta).
    ///
    /// # Errors
    ///
    /// `Err` means **the parser could not be built** — what the factory needs
    /// (configuration, definition data, an embedding's runtime) is broken or
    /// unavailable — and **aborts the parse** under any recovery policy; the
    /// descent site attaches the live traceback when the error carries no frames
    /// of its own. Refusing to parse *deeper* is deliberately not this channel's
    /// business: nesting depth belongs to the descent guard, which refuses with
    /// [`DescentLimitExceeded`](crate::constructs::DescentLimitExceeded) inside
    /// [`ParseContext::parse_construct`](crate::constructs::ParseContext::parse_construct),
    /// before any factory-built parser runs. Carry
    /// [`HookFailed`](crate::error::HookFailed) for an operational failure,
    /// [`ImplementationError`](crate::constructs::ImplementationError) for a
    /// violated library contract. An infallible implementation wraps its parser
    /// in `Ok(...)` and that is the only change.
    // The boxed-parser-or-abort pair is the decided factory signature; an alias
    // would only rename it.
    #[allow(clippy::type_complexity)]
    fn make_nodes_parser<'p>(
        &'p self,
        stop: StopSpec<'p, L>,
        child_states: ChildStateSpec<'p, L>,
    ) -> Result<
        Box<dyn ConstructParser<L, Output = NodesOutcome<L>> + 'p>,
        ParseError<L::SourceOrigin>,
    >
    where
        L::InvocationSyntax: FromInvocation<L>,
    {
        Ok(Box::new(NodesParser::new(stop).with_child_states(child_states)))
    }

    /// The factory producing the parser for one group descent (the consumed
    /// `GroupOpen` token and its resolved rule) — a fresh boxed parser
    /// per descent. Reached through [`ParseContext::parse_group`](crate::constructs::ParseContext::parse_group) at every group
    /// descent site.
    ///
    /// The default is the standard [`GroupParser`], which derives the interior state
    /// through [`ParseContext::group_interior_state`](crate::constructs::ParseContext::group_interior_state) (where
    /// [`group_interior_delta`](ParseDriver::group_interior_delta) merges in) — prefer
    /// the delta channel for state-shaped customization; override this factory only
    /// for structurally different group parses.
    ///
    /// # Errors
    ///
    /// `Err` means **the parser could not be built** and aborts the parse under
    /// any recovery policy — never a depth refusal, which stays the descent
    /// guard's business; the condition choice and the full contract are
    /// [`make_nodes_parser`](ParseDriver::make_nodes_parser)'s. An infallible
    /// implementation wraps its parser in `Ok(...)` and that is the only change.
    // The decided factory signature, as for `make_nodes_parser`.
    #[allow(clippy::type_complexity)]
    fn make_group_parser<'p>(
        &'p self,
        open: &L::Token,
        rule: Arc<GroupRule<L>>,
        child_states: ChildStateSpec<'p, L>,
    ) -> Result<
        Box<dyn ConstructParser<L, Output = BuildId> + 'p>,
        ParseError<L::SourceOrigin>,
    >
    where
        L::InvocationSyntax: FromInvocation<L>,
    {
        Ok(Box::new(
            GroupParser::new(open.clone(), rule).with_child_states(child_states),
        ))
    }

    /// The interception point over
    /// [`CallableSpec::make_invocation_parser`]: the dispatch loops obtain every
    /// invocation parser through the driver, and the default delegates to the resolved
    /// spec's own factory (`invocation.spec` — it builds no parser itself and calls
    /// no other driver method) — specs keep owning their invocation behavior; the driver
    /// merely gets a uniform veto/wrap point (instrumentation, per-language parser
    /// substitution) that no per-spec override could provide.
    ///
    /// The caller has already consumed the trigger token whole; see
    /// [`StdInvocationParser`](crate::constructs::StdInvocationParser)'s
    /// documentation for the invocation-parser contract an implementation must
    /// uphold.
    ///
    /// # Errors
    ///
    /// `Err` means **the parser could not be built** and aborts the parse under
    /// any recovery policy — never a depth refusal, which stays the descent
    /// guard's business; the condition choice and the full contract are
    /// [`make_nodes_parser`](ParseDriver::make_nodes_parser)'s. The default
    /// passes the spec factory's own answer through unchanged
    /// ([`CallableSpec::make_invocation_parser`]'s `Err` included). An infallible
    /// implementation wraps its parser in `Ok(...)` and that is the only change.
    // The decided factory signature, as for `make_nodes_parser`.
    #[allow(clippy::type_complexity)]
    fn make_invocation_parser<'a>(
        &'a self,
        invocation: Invocation<'a, L>,
    ) -> Result<
        Box<dyn ConstructParser<L, Output = BuildId> + 'a>,
        ParseError<L::SourceOrigin>,
    >
    where
        L::InvocationSyntax: FromInvocation<L>,
    {
        let spec = invocation.spec;
        spec.make_invocation_parser(invocation)
    }
}

/// The pluggable body of [`ParseDriver::resolve_command`] — the strategy value
/// carried by [`StdParseDriver`], so command resolution plugs into the one ready-made
/// driver instead of one driver struct existing per behavior.
///
/// `resolve_command` is deliberately the **only** [`ParseDriver`] hook with a
/// pluggable strategy: it is the sole hook that is both non-defaultable for a real
/// command-bearing language (the core cannot invent the language's command
/// [`CallableTypeId`](Lang::CallableTypeId)) and has more than one ready-made behavior
/// worth shipping. No other hook grows one; a language outgrowing the ready-made
/// strategies writes its own [`ParseDriver`] — the normal path.
///
/// Shipped strategies: `()` resolves nothing (the [`StdParseDriver`] default —
/// test languages and languages without commands);
/// [`ScopesCommandResolver`] is the standard scope-stack resolution
/// ([`resolve_command_in_scopes`]) under a fixed command callable type.
pub trait CommandResolver<L: Lang>: fmt::Debug + Send + Sync {
    /// Resolve a [`Command`](TokenKind::Command) token, reading it through the
    /// reader that produced it — the contract, the meaning of an `Err` (abort) versus
    /// a [`Failed`](CommandResolution::Failed) resolution (diagnose and recover),
    /// and the condition choice are [`ParseDriver::resolve_command`]'s, which
    /// [`StdParseDriver`] forwards here. The two signatures stay in step.
    fn resolve_command(
        &self,
        state: &ParsingState<L>,
        token: &L::Token,
        tokens: &dyn TokenReader<'_, L>,
    ) -> Result<CommandResolution<L>, ParseError<L::SourceOrigin>>;
}

/// The no-op resolver: resolves nothing, with a detail reporting that command
/// resolution is not implemented (so a language that enables commands without
/// configuring resolution sees the actual cause, not a bare "cannot resolve").
impl<L: Lang> CommandResolver<L> for () {
    fn resolve_command(
        &self,
        state: &ParsingState<L>,
        token: &L::Token,
        tokens: &dyn TokenReader<'_, L>,
    ) -> Result<CommandResolution<L>, ParseError<L::SourceOrigin>> {
        let _ = (state, token, tokens);
        Ok(CommandResolution::Unresolved {
            detail: Some(
                "command resolution is not implemented by this language’s driver — \
                 implement ‘ParseDriver::resolve_command’ or use a preset"
                    .into(),
            ),
        })
    }
}

/// The standard scope-stack [`CommandResolver`] strategy: every command token
/// resolves through [`resolve_command_in_scopes`] under the one fixed
/// [`command_type`](ScopesCommandResolver::command_type). The field **is** the datum
/// core cannot default — a language's command [`CallableTypeId`](Lang::CallableTypeId)
/// (contrast specials, where the provider supplies the resolved type with the match);
/// languages with several command-syntax callable types write their own resolver —
/// that is what the strategy point exists for.
///
/// Public home: `techy::core::specs`, beside [`resolve_command_in_scopes`] and the
/// resolution family it packages.
pub struct ScopesCommandResolver<L: Lang> {
    /// The callable type every command token resolves under (the latexlike preset's
    /// analogue is its macro callable type).
    pub command_type: L::CallableTypeId,
}

impl<L: Lang> CommandResolver<L> for ScopesCommandResolver<L> {
    fn resolve_command(
        &self,
        state: &ParsingState<L>,
        token: &L::Token,
        tokens: &dyn TokenReader<'_, L>,
    ) -> Result<CommandResolution<L>, ParseError<L::SourceOrigin>> {
        Ok(resolve_command_in_scopes(state, token, tokens, self.command_type))
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug` although only the
// `CallableTypeId` associated type (already `Copy` + `Debug`) is stored.

impl<L: Lang> Clone for ScopesCommandResolver<L> {
    fn clone(&self) -> Self {
        ScopesCommandResolver { command_type: self.command_type }
    }
}

impl<L: Lang> fmt::Debug for ScopesCommandResolver<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopesCommandResolver")
            .field("command_type", &self.command_type)
            .finish()
    }
}

/// The one ready-made [`ParseDriver`]: the [`Recovery`] policy setting, a pluggable
/// [`CommandResolver`] strategy, and an optional [`SourceResolver`] — everything else
/// keeps the trait defaults. It implements the trait for **every** language whose
/// [`SourceOrigin`](Lang::SourceOrigin) is `O` and whose commands `R` can resolve —
/// the [`TrivialLang`](crate::state::TrivialLang) default driver (`type Driver =
/// StdParseDriver`, all parameters defaulted).
///
/// **Which command resolver to reach for:** `()` resolves nothing — the
/// [`TrivialLang`](crate::state::TrivialLang)-style test-language pairing
/// (`StdParseDriver::new(Recovery::Strict, ())`), and right for languages without
/// command syntax; [`ScopesCommandResolver`]
/// resolves every command through the state's scope stack under one fixed callable
/// type — the standard shape for a command-bearing language; beyond those, implement
/// [`CommandResolver`] (or a whole [`ParseDriver`]) yourself.
///
/// # The two resolvers are deliberately asymmetric
///
/// Storage matches how each part is consumed. The **command resolver** is part of the
/// language *definition* — fixed when `type Driver = …` is written — and is consumed
/// monomorphized through the concretely-typed
/// [`ParseContext::driver`](crate::constructs::ParseContext::driver) on every
/// command token — a performance-critical path — so a generic parameter is
/// collected in full. The
/// **source resolver** is an *embedding-environment* capability — it varies per
/// deployment or run — and is consumed only through the type-erased
/// [`ParseDriver::source_resolver`] accessor once per `\input`-style inclusion —
/// far off any performance-critical path: a
/// generic parameter there would be erased at its only point of use, while costing a
/// none-placeholder type and `None`-inference noise. Hence `R` by value,
/// [`source_resolver`](StdParseDriver::source_resolver) behind `Option<Arc<dyn …>>`.
///
/// ```
/// # use techy::core::StdParseDriver;
/// # use techy::error::Recovery;
/// let strict: StdParseDriver = StdParseDriver::new(Recovery::Strict, ());
/// assert_eq!(strict.recovery, Recovery::Strict);
/// let tolerant: StdParseDriver = StdParseDriver::new(Recovery::Tolerant, ());
/// # let _ = tolerant;
/// ```
pub struct StdParseDriver<R = (), O: SourceOrigin = Option<String>> {
    /// The tolerant-parsing policy to drive under.
    pub recovery: Recovery,
    // The ruled asymmetry (storage matches the consumption seam — see the type-level
    // docs): the command resolver is language-definition data, consumed monomorphized
    // on the per-command-token hot path — a by-value generic, collected in full; the
    // source resolver is an embedding-environment capability, consumed only through
    // the type-erased `ParseDriver::source_resolver` accessor on the once-per-`\input`
    // cold path — value-level `dyn` behind `Option<Arc<…>>`, `None` = resolves
    // nothing.
    /// The [`CommandResolver`] strategy behind
    /// [`ParseDriver::resolve_command`] (`()` = resolves nothing).
    pub command_resolver: R,
    /// The [`SourceResolver`] behind [`ParseDriver::source_resolver`]
    /// (`None` = this language resolves no external source references); set via
    /// [`with_source_resolver`](StdParseDriver::with_source_resolver).
    pub source_resolver: Option<Arc<dyn SourceResolver<O>>>,
}

impl<R, O: SourceOrigin> StdParseDriver<R, O> {
    /// A driver with the given recovery policy and command resolver (`()` = resolves
    /// nothing), and no source resolver.
    pub fn new(recovery: Recovery, command_resolver: R) -> StdParseDriver<R, O> {
        StdParseDriver { recovery, command_resolver, source_resolver: None }
    }

    /// Use `resolver` for `\input`-like external source references — exposed through
    /// [`ParseDriver::source_resolver`]. Takes a resolver by value (shared internally)
    /// or an already-shared `Arc` (passed through, no double-wrap).
    pub fn with_source_resolver<M>(
        mut self,
        resolver: impl IntoSourceResolver<O, M>,
    ) -> StdParseDriver<R, O> {
        self.source_resolver = Some(resolver.into_source_resolver());
        self
    }
}

// The `Token`/`StreamPosition` bounds are what let the ready-made driver build the
// ready-made reader: `StdTokenReader` serves exactly the languages whose tokens are
// `StdToken` and whose stream positions are `StdStreamPosition`. A language with its
// own token or position type brings its own driver.
impl<L, R: CommandResolver<L>> ParseDriver<L> for StdParseDriver<R, L::SourceOrigin>
where
    L: Lang<Token = StdToken<L>, StreamPosition = StdStreamPosition>,
{
    fn make_token_reader<'s>(
        &'s self,
        source: &'s Arc<Source<L::SourceOrigin>>,
    ) -> Box<dyn TokenReader<'s, L> + 's> {
        Box::new(StdTokenReader::new(source))
    }

    fn recovery(&self) -> Recovery {
        self.recovery
    }

    /// Forwards to the driver's [`command_resolver`](StdParseDriver::command_resolver)
    /// strategy.
    fn resolve_command(
        &self,
        state: &ParsingState<L>,
        token: &L::Token,
        tokens: &dyn TokenReader<'_, L>,
    ) -> Result<CommandResolution<L>, ParseError<L::SourceOrigin>> {
        self.command_resolver.resolve_command(state, token, tokens)
    }

    fn source_resolver(&self) -> Option<&dyn SourceResolver<L::SourceOrigin>> {
        self.source_resolver.as_deref()
    }
}

// Manual impls: a `Clone` derive would spuriously demand `O: Clone` (the `Arc` field
// clones by refcount), and a `Debug` derive would demand a `Debug` bound the
// `dyn SourceResolver` cannot supply — that field is shown by presence only.
// Driver `Copy`/`Eq` are deliberately gone (API-review T4 ruling): nothing relied on
// them, and the resolver fields are not `Copy`/`Eq` material.

impl<R: Clone, O: SourceOrigin> Clone for StdParseDriver<R, O> {
    fn clone(&self) -> Self {
        StdParseDriver {
            recovery: self.recovery,
            command_resolver: self.command_resolver.clone(),
            source_resolver: self.source_resolver.clone(),
        }
    }
}

impl<R: fmt::Debug, O: SourceOrigin> fmt::Debug for StdParseDriver<R, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StdParseDriver")
            .field("recovery", &self.recovery)
            .field("command_resolver", &self.command_resolver)
            .finish_non_exhaustive()
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
    /// The command did not resolve — a clean miss (the name is defined nowhere the
    /// query could see); the parse loops diagnose it as unresolvable and recover
    /// (span-backed chars fallback).
    Unresolved {
        /// Optional human-facing detail on why resolution failed, appended to the
        /// diagnostic's message and serialized with the condition. `None` when there
        /// is nothing to say beyond "the name did not resolve".
        detail: Option<String>,
    },
    /// Resolution failed *operationally* — a definition provider errored while
    /// answering the query (a broken or unavailable source), as opposed to a clean
    /// miss. Diagnosed as a distinct condition
    /// ([`CommandResolutionFailed`](crate::constructs::CommandResolutionFailed)) so
    /// tooling can tell "command unknown" from "resolver broken"; recovery is the same
    /// span-backed chars fallback.
    Failed {
        /// Optional human-facing detail on the operational failure (typically the
        /// provider's rendered error), appended to the diagnostic's message.
        detail: Option<String>,
    },
}

/// The standard "resolve a command through the scope stack" body, under the given
/// `callable_type` — the single home for every driver's
/// [`resolve_command`](ParseDriver::resolve_command) that dispatches to the state's
/// scope stack (the latexlike preset and the test langs), so query construction and
/// the miss/failure detail policy live in one place rather than drifting per copy.
/// [`ScopesCommandResolver`] is its one-line packaging as the [`CommandResolver`]
/// strategy for [`StdParseDriver`].
///
/// Builds a [`CallableQuery`] with
/// [`CallableSyntax::Command`](crate::scopes::CallableSyntax::Command) (the token's
/// fired escape character), consults
/// [`ScopeStack::retrieve_spec`](crate::scopes::ScopeStack::retrieve_spec), and maps
/// the outcome: a hit is [`Resolved`](CommandResolution::Resolved); a clean miss is
/// [`Unresolved`](CommandResolution::Unresolved) carrying the searched providers —
/// and, where the scopes advertise their symbols, a **did-you-mean** hint — as
/// detail; an operational provider error is [`Failed`](CommandResolution::Failed)
/// carrying the provider's rendered error. The token is read through `tokens`, the
/// reader that produced it; a non-[`Command`](TokenKind::Command) token — a
/// caller-contract violation — yields `Unresolved { detail: None }`.
///
/// **The did-you-mean detail** scans the providers' advertised definitions
/// ([`SpecsProvider::iter_symbols`](crate::scopes::SpecsProvider::iter_symbols),
/// under the queried callable type and the state's current mode) for near-misses
/// of the unresolved name: a definition registered *with* its escape character
/// (`\greet` instead of `greet` — the registration trap
/// [`Package::insert`](crate::scopes::Package::insert)'s contract warns about) is
/// called out explicitly, and small-edit-distance names are suggested. Providers
/// that cannot enumerate are skipped, and an in-stack fallback provider makes
/// resolution *succeed*, so the miss path — hints included — never runs there
/// (accepted limitation; the parse-initialization check
/// `check_provider_commands_shadowed_by_escape` fires regardless of fallbacks).
pub fn resolve_command_in_scopes<L: Lang>(
    state: &ParsingState<L>,
    token: &L::Token,
    tokens: &dyn TokenReader<'_, L>,
    callable_type: L::CallableTypeId,
) -> CommandResolution<L> {
    let TokenKind::Command { name, escape_char } = tokens.token_kind(token) else {
        return CommandResolution::Unresolved { detail: None };
    };
    let query =
        CallableQuery::new(callable_type, name, CallableSyntax::Command { escape_char });
    match state.scopes().retrieve_spec(&query, state) {
        Ok(Some(spec)) => {
            CommandResolution::Resolved(ResolvedCallable { callable_type, spec })
        }
        Ok(None) => {
            let mut detail = state.scopes().searched_providers().to_string();
            if let Some(hint) = did_you_mean_hint(state, name, escape_char, callable_type)
            {
                detail.push_str("; ");
                detail.push_str(&hint);
            }
            CommandResolution::Unresolved { detail: Some(detail) }
        }
        Err(error) => CommandResolution::Failed { detail: Some(error.to_string()) },
    }
}

/// Compose the did-you-mean miss detail (see [`resolve_command_in_scopes`]): scan
/// the enumerable providers innermost-first for (a) the unresolved name registered
/// *with* its escape character and (b) small-edit-distance near-misses. `None`
/// when nothing nearby is advertised.
fn did_you_mean_hint<L: Lang>(
    state: &ParsingState<L>,
    name: &str,
    escape_char: char,
    callable_type: L::CallableTypeId,
) -> Option<String> {
    /// At most this many near-miss suggestions (innermost-first).
    const MAX_SUGGESTIONS: usize = 3;

    let mut escape_trap: Option<String> = None; // the defining provider's name
    let mut suggestions: Vec<(String, String)> = Vec::new(); // (provider, name)
    let escape_spelling = {
        let mut s = String::with_capacity(escape_char.len_utf8() + name.len());
        s.push(escape_char);
        s.push_str(name);
        s
    };
    let threshold = if name.chars().count() <= 4 { 1 } else { 2 };

    for provider in state.scopes().providers().iter().rev() {
        let Some(symbols) = provider.iter_symbols(callable_type, state.mode()) else {
            continue; // cannot enumerate (e.g. a fallback provider): skipped
        };
        for entry in symbols {
            if escape_trap.is_none() && entry.name == escape_spelling {
                escape_trap = Some(provider.name().into());
            } else if suggestions.len() < MAX_SUGGESTIONS
                && entry.name != name
                && !suggestions.iter().any(|(_, n)| n == entry.name)
                && edit_distance_at_most(entry.name, name, threshold)
            {
                suggestions.push((provider.name().into(), entry.name.into()));
            }
        }
    }

    let mut hint = String::new();
    if let Some(provider) = escape_trap {
        hint.push_str(&format!(
            "provider ‘{provider}’ defines ‘{escape_spelling}’ — command names are \
             registered without the escape character"
        ));
    }
    if !suggestions.is_empty() {
        if !hint.is_empty() {
            hint.push_str("; ");
        }
        hint.push_str("did you mean ");
        for (i, (provider, suggested)) in suggestions.iter().enumerate() {
            if i > 0 {
                hint.push_str(" or ");
            }
            hint.push_str(&format!("‘{suggested}’ (provider ‘{provider}’)"));
        }
        hint.push('?');
    }
    (!hint.is_empty()).then_some(hint)
}

/// Whether the Levenshtein distance of `a` and `b` is `<= bound` (char-based;
/// two-row dynamic programming with an early length gate).
fn edit_distance_at_most(a: &str, b: &str, bound: usize) -> bool {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > bound {
        return false;
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        current[0] = i + 1;
        let mut row_min = current[0];
        for (j, &cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != cb);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
            row_min = row_min.min(current[j + 1]);
        }
        if row_min > bound {
            return false; // every path already exceeds the bound
        }
        core::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()] <= bound
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
            CommandResolution::Failed { detail } => {
                CommandResolution::Failed { detail: detail.clone() }
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
            CommandResolution::Failed { detail } => {
                f.debug_struct("Failed").field("detail", detail).finish()
            }
        }
    }
}
