//! [`ParseDriver`]: a language's parse-time behavior, as methods on a value.
//!
//! [`Lang`](crate::core::Lang) stays the compile-time bundle: its hooks belong to
//! layers callable outside a running parse (state transitions, tokenizer specials,
//! node finalization). Everything that only runs *while a parse is running* is on the
//! driver instead, as `&self` methods on a value, so the behavior can carry
//! configuration that a static `Lang` hook could not (a recovery policy, a preset's
//! package registry).
//!
//! The driver covers five concerns:
//!
//! - **policy** — the [`Recovery`] setting ([`recovery`](ParseDriver::recovery)) and
//!   the two methods consulting it ([`recover`](ParseDriver::recover),
//!   [`probe_token`](ParseDriver::probe_token));
//! - **parse-time hooks** —
//!   [`resolve_command`](ParseDriver::resolve_command),
//!   [`make_paragraph_break_node`](ParseDriver::make_paragraph_break_node),
//!   [`refine_diagnostic`](ParseDriver::refine_diagnostic),
//!   [`observe_transition`](ParseDriver::observe_transition),
//!   [`resolve_state_event`](ParseDriver::resolve_state_event);
//! - **source resolution** — the
//!   [`source_resolver`](ParseDriver::source_resolver) accessor exposing the
//!   embedding environment's [`SourceResolver`] for `\input`-like external
//!   references (`None` = resolves nothing);
//! - **the group descent delta** —
//!   [`group_interior_delta`](ParseDriver::group_interior_delta), through which a
//!   group class changes the parsing state of its interior (a math group entering
//!   math mode is one line: a delta with a
//!   [`mode`](crate::core::ParsingStateDelta::mode) override);
//! - **construct provision** — [`make_root_parser`](ParseDriver::make_root_parser),
//!   [`make_nodes_parser`](ParseDriver::make_nodes_parser),
//!   [`make_group_parser`](ParseDriver::make_group_parser),
//!   [`make_invocation_parser`](ParseDriver::make_invocation_parser). Every descent
//!   site goes through the [`ParseContext`](crate::core::constructs::ParseContext) wrappers
//!   ([`parse_nodes`](crate::core::constructs::ParseContext::parse_nodes)/[`parse_group`](crate::core::constructs::ParseContext::parse_group)),
//!   so one override applies uniformly to the whole parse; the root parser is what
//!   the parse entry point ([`ParseSetup::parse`](super::ParseSetup::parse)) runs
//!   directly.
//!
//! Drivers are shared and immutable (`&self`, `Send + Sync`): mutable per-parse state
//! belongs to the [`ParserSession`] (which owns the derivation memos the
//! driver-consulting helpers [`ParserSession::derived_state`] and
//! [`ParserSession::group_interior_state`] use), and per-language *data* belongs to
//! the parsing state. [`StdParseDriver`] is the one ready-made implementation — the
//! recovery setting, a pluggable [`CommandResolver`] strategy, an optional source
//! resolver — and the [`TrivialLang`](crate::core::TrivialLang) default.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::constructs::{
    ChildStateSpec, ConstructParser, ConstructParserResult, FromInvocation, GroupParser,
    Invocation, NodesOutcome, NodesParser, RootNodesParser, StopSpec,
};
use crate::error::{DiagnosticData, Diagnostics, ParseError, Recovery};
use crate::node::{BuildId, NodeKind};
use crate::scopes::{CallableQuery, CallableSyntax};
use crate::source::{
    IntoSourceResolver, Source, SourceOrigin, SourceResolver, SourceSpan,
};
use crate::spec::CallableSpec;
use crate::state::{Lang, ParsingState, ParsingStateDelta, ParsingStateStack};
use crate::token::{GroupRule, Token, TokenKind, TokenReader, Tokenization};


use super::ParserSession;

/// A language's parse-time behavior: how commands resolve, whether a problem aborts
/// the parse or is recorded and tolerated, and which parsers run at each step.
///
/// The engine consults the driver at fixed points of every parse. A language names
/// its driver type as [`Lang::Driver`](crate::core::Lang::Driver), an instance is
/// stored on the [`Language`](super::Language), and construct parsers reach it as
/// [`ParseContext::driver`](crate::core::constructs::ParseContext::driver) — concretely
/// typed through `L`, so a preset's own parsers can call inherent methods on the
/// driver type without downcasting, while generic code sees only this trait.
/// [Defining a language](crate::guide::custom_lang#the-driver) walks through writing
/// one.
///
/// [`StdParseDriver`] is the ready-made implementation: a recovery policy, a
/// pluggable [`CommandResolver`], and an optional source resolver. Many languages
/// need nothing else.
///
/// # Which methods to implement
///
/// **Every method has a default**, so `impl ParseDriver<MyLang> for MyDriver {}` is
/// already a complete driver — one that resolves no commands, aborts at the first
/// problem, and uses the standard parsers everywhere. Tokenization is covered too:
/// [`make_token_reader`](ParseDriver::make_token_reader) takes the reader from the
/// language's [`Lang::Tokenization`](crate::core::Lang::Tokenization) by default.
///
/// What a real language usually overrides, and when the engine calls it:
///
/// - [`resolve_command`](ParseDriver::resolve_command) — **required for any language
///   with command syntax.** The core cannot invent the language's callable type
///   identifiers, and the default resolves nothing, so every command in the document
///   would be diagnosed as unresolvable. Called once per command token.
/// - [`recovery`](ParseDriver::recovery) — the strict-or-tolerant policy. The default
///   is [`Recovery::Strict`]; read whenever a problem is detected and on every token
///   probe.
/// - [`group_interior_delta`](ParseDriver::group_interior_delta) — set when entering a
///   group of some class changes the state its interior is parsed under, such as a
///   math group switching the mode. Called once per distinct base state and group
///   rule, since the result is memoized for the parse.
/// - [`refine_diagnostic`](ParseDriver::refine_diagnostic) — replace a condition with
///   a language-specific one before it is recorded. Called once per detected problem
///   by the default [`recover`](ParseDriver::recover).
/// - the `make_*` parser factories — supply parsers of the language's own for the
///   root, for content runs, for groups, and for invocations. Called once per descent
///   of the corresponding kind.
/// - [`observe_parse_start`](ParseDriver::observe_parse_start) and
///   [`observe_transition`](ParseDriver::observe_transition) — record what a parse
///   did without changing it. The first is called once per parse before any token is
///   read; the second on every state transition.
/// - [`source_resolver`](ParseDriver::source_resolver) — supply the resolver for
///   `\input`-like external references. Called once per such inclusion.
/// - [`resolve_state_event`](ParseDriver::resolve_state_event) — turn a
///   context-dependent transition event into an ordinary state patch. Called once per
///   such event.
/// - [`diagnostics_limit`](ParseDriver::diagnostics_limit) — cap how many diagnostics
///   one parse retains. Called once per parse.
///
/// How deeply a parse may nest is deliberately *not* a driver concern: the engine
/// owns that limit through [`StdDescentGuard`](super::StdDescentGuard), configured on
/// the [`Language`](super::Language) with
/// [`with_descent_guard_init`](super::Language::with_descent_guard_init).
///
/// # Instances are shared and hold no parse state
///
/// Every method takes `&self`, and one driver instance is shared across parses and
/// across threads (`Send + Sync`). A driver may carry configuration, but never state
/// belonging to a single parse — that belongs to the [`ParserSession`]. Hooks the
/// session memoizes ([`group_interior_delta`](ParseDriver::group_interior_delta))
/// must in addition be pure functions of their arguments; the per-method
/// documentation states each contract.
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

    /// The recovery policy this driver parses under: abort at the first problem, or
    /// record it and continue.
    ///
    /// The default is [`Recovery::Strict`]; [`StdParseDriver`] carries the policy as
    /// a field. It is read by the default [`recover`](ParseDriver::recover) and
    /// [`probe_token`](ParseDriver::probe_token) paths, so a policy finer than the
    /// two-way choice overrides those methods instead of this one.
    ///
    /// Deliberately infallible: this is a pure read of configured policy, with no
    /// computation that could fail. Embedding or binding code whose implementation
    /// can still fail should report the failure through the embedding's own channel
    /// and answer [`Recovery::Strict`] — the conservative policy: a strict parse
    /// aborts on problems instead of continuing under a policy nobody chose.
    fn recovery(&self) -> Recovery {
        Recovery::Strict
    }

    /// How many diagnostics one parse retains, or `None` — the default — for the
    /// standard cap ([`Diagnostics::DEFAULT_LIMIT`]).
    ///
    /// Read once per parse: [`ParseSetup::parse`](super::ParseSetup::parse) creates
    /// the session's [`Diagnostics`] with [`Diagnostics::with_limit`] when this
    /// answers `Some(limit)`.
    ///
    /// Code that drives construct parsers over a [`ParserSession`] of its own
    /// applies the cap itself; the session's
    /// [`diagnostics`](ParserSession::diagnostics) field is public.
    fn diagnostics_limit(&self) -> Option<usize> {
        None
    }

    /// Decides what happens when a construct parser detects a problem: record it and
    /// continue, or end the parse.
    ///
    /// Called once per detected problem, from the parsers' recovery entry point
    /// [`ParseContext::recover`](crate::core::constructs::ParseContext::recover).
    ///
    /// The default applies [`refine_diagnostic`](ParseDriver::refine_diagnostic)
    /// exactly once, then follows the policy
    /// [`recovery`](ParseDriver::recovery) answers: under
    /// [`Recovery::Tolerant`] it records the condition as an error-severity
    /// diagnostic and returns `Ok(())`, so the caller continues with the local
    /// recovery documented for that condition; under [`Recovery::Strict`] it returns
    /// the condition as a [`ParseError`], and nothing continues past that. Both calls
    /// go **through `self`**, which a delegating driver must account for — see
    /// *Wrapping a driver* on the trait.
    ///
    /// Overriding this method replaces the policy, not the two calls it is made of. A
    /// richer policy
    /// — per-condition severities, a list of conditions to ignore, a budget of
    /// problems — decides per call between [`ParserSession::recover`]'s two modes. An
    /// override also takes on the refinement step: either route the condition through
    /// [`refine_diagnostic`](ParseDriver::refine_diagnostic) before recording it, or
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

    /// Builds the token reader one parse reads `source` through.
    ///
    /// This is where a driver installs a reader of its own. Both
    /// reader-construction sites go through it:
    /// [`ParseSetup::parse`](super::ParseSetup::parse) for the root parse and
    /// [`ParseContext::parse_attached_source`](crate::core::constructs::ParseContext::parse_attached_source)
    /// for an attached (included) source. A driver that returns its own reader thereby
    /// tokenizes the whole parse its way, while the *types* involved stay fixed by the
    /// language ([`Token<L>`](crate::core::token::Token) and
    /// [`StreamPosition<L>`](crate::core::token::StreamPosition), both declared by
    /// [`Lang::Tokenization`](crate::core::Lang::Tokenization)).
    ///
    /// The default builds the reader the language's own
    /// [`Tokenization`](crate::core::token::Tokenization) names —
    /// `L::Tokenization::make_token_reader(source)`, which for
    /// [`StdTokenization`](crate::core::token::StdTokenization) is a
    /// [`StdTokenReader`](crate::core::token::StdTokenReader) over `source`. Override it when
    /// the reader needs data the driver *instance* holds: the returned reader borrows
    /// both `self` and `source` for the parse's extent, so a driver may pass it
    /// configuration of its own. A reader needing no per-instance data is better
    /// installed by the language, as its `Tokenization`.
    fn make_token_reader<'s>(
        &'s self,
        source: &'s Arc<Source<L::SourceOrigin>>,
    ) -> Box<dyn TokenReader<'s, L> + 's> {
        <L::Tokenization as Tokenization<L>>::make_token_reader(source)
    }

    /// Looks at the token at the reader's position under `state` without consuming
    /// it, mapping a tokenizer error to the recovery policy.
    ///
    /// This is the peek an argument parser uses to find out whether an optional
    /// argument is present, reached through
    /// [`ParseContext::probe_token`](crate::core::constructs::ParseContext::probe_token).
    /// The default reads the policy through
    /// `self.`[`recovery()`](ParseDriver::recovery): strict mode aborts with the
    /// token error, as the content loop would; tolerant mode reports `None` **without
    /// diagnosing and without consuming**, so the caller treats the position as
    /// unusable (the argument is absent, the terminator malformed) and the enclosing
    /// content loop re-reads the error and applies its own token recovery. That is
    /// what keeps one malformed token from being reported twice.
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
    ) -> ConstructParserResult<L, Option<Token<L>>> {
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

    // --- parse-time hooks ---------------------------------------------------------

    /// Resolves a [`Command`](crate::core::token::TokenKind::Command) token to its
    /// invocation form and behavior spec.
    ///
    /// Called once per command token. A preset typically implements it by querying
    /// the state's definitions with a
    /// [`CallableQuery`](crate::core::specs::CallableQuery), which carries the name
    /// and the escape character that fired, so that the syntax can be told apart.
    ///
    /// `Specials` tokens need no such hook: recognizing one already resolves it, and
    /// the token carries its spec. Specials are resolved while tokenizing, by
    /// [`Lang::scan_specials`](crate::core::Lang::scan_specials); commands are
    /// resolved while parsing, here.
    ///
    /// The hook receives the triggering **token** and a shared, call-scoped reference
    /// to the **reader that produced it**, so a language may take over resolution by
    /// inspecting any detail of the token it needs:
    /// [`tokens.token_kind(token)`](crate::core::token::TokenReader::token_kind) for what it
    /// is, [`source_span_of`](crate::core::token::TokenReader::source_span_of) or
    /// [`position_at`](crate::core::token::TokenReader::position_at) for where it is. The
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
    ///   ([`CommandResolutionFailed`](crate::core::constructs::CommandResolutionFailed))
    ///   and continues with the span-backed chars recovery.
    /// - `Err` is the **abort** channel: carry
    ///   [`HookFailed`](crate::error::HookFailed) for an operational failure the
    ///   parse must not continue past (a resolver backend that is down for the whole
    ///   parse, a runtime failure in an embedding),
    ///   [`ImplementationError`](crate::core::constructs::ImplementationError) for a
    ///   violated library contract, or a document condition only for a diagnosis
    ///   made deliberately (aborting is strict-mode behavior; recoverable document
    ///   problems belong on the `Ok` channels).
    ///
    /// An infallible implementation wraps its resolution in `Ok(...)` and that is
    /// the only change.
    fn resolve_command(
        &self,
        state: &ParsingState<L>,
        token: &Token<L>,
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
    /// [`ImplementationError`](crate::core::constructs::ImplementationError).
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

    /// Records what a state transition did, without being able to change it.
    ///
    /// Called by the session's derivation helpers
    /// ([`ParserSession::derived_state`], [`ParserSession::group_interior_state`]) on
    /// **every** transition — memoized ones included, which is what
    /// [`Lang::finalize_transition`](crate::core::Lang::finalize_transition)
    /// structurally cannot see (it runs once per unique *derivation*, not once per
    /// transition). Parse-history accumulation ("how many times did the parse enter
    /// math mode") belongs here, in the session's
    /// [`SessionExt`](crate::core::Lang::SessionExt) — never in
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
    ///   [`ImplementationError`](crate::core::constructs::ImplementationError) for a
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

    /// Records observations about a parse before it starts.
    ///
    /// Called once per parse by
    /// [`ParseSetup::parse`](crate::core::ParseSetup::parse), after the session is
    /// created and before any token is read. That is the right moment to check that
    /// the definitions loaded for this parse make sense: the diagnostics sink already
    /// exists, and the initial state's
    /// [`TokenRules`](crate::core::token::TokenRules) — escape characters included —
    /// are known, which nothing at registration time can see.
    ///
    /// `initial_state` is the state **this parse** starts from: the language's
    /// initial state, or whatever
    /// [`ParseSetup::with_initial_state`](crate::core::ParseSetup::with_initial_state)
    /// put in its place.
    ///
    /// The hook returns nothing, so it cannot fail and cannot stop the parse: whatever
    /// it records, the parse goes on. Record warnings and notes only. Nothing prevents
    /// pushing an entry of `Error` severity, but such an entry is kept and counted like
    /// any other — [`Diagnostics::has_errors`](crate::error::Diagnostics::has_errors)
    /// answers `true` afterwards — while the parse still runs to completion, and the
    /// strict recovery policy never sees it (this hook does not go through
    /// [`recover`](ParseDriver::recover)). A consumer would be handed a result that both
    /// completed and reports errors, with nothing to explain the difference. The default
    /// does nothing.
    ///
    /// The latexlike driver delegates to
    /// [`LatexlikeLang::check_parse_start`](crate::latexlike::LatexlikeLang::check_parse_start),
    /// which for the shipped preset runs the all-escape-shadowed provider check
    /// ([`check_provider_commands_shadowed_by_escape`](crate::core::specs::check_provider_commands_shadowed_by_escape)).
    ///
    /// Attached-source sub-parses
    /// ([`parse_attached_source`](crate::core::constructs::ParseContext::parse_attached_source))
    /// deliberately do **not** re-fire this hook: it observes *parse
    /// initialization* (the seeded providers), not every descent.
    fn observe_parse_start(
        &self,
        source: &Arc<Source<L::SourceOrigin>>,
        initial_state: &Arc<ParsingState<L>>,
        diagnostics: &mut Diagnostics<L::SourceOrigin>,
    ) {
        let _ = (source, initial_state, diagnostics);
    }

    /// Turns one **context-dependent** transition event into an ordinary state-delta
    /// patch, using the states the parse has descended through.
    ///
    /// Events come in two classes ([`Lang::Event`]); this method handles the class
    /// whose meaning depends on the surrounding context. It is consulted by
    /// [`ParseContext::derive_state`](crate::core::constructs::ParseContext::derive_state)
    /// once per event before the delta reaches the derivation point
    /// ([`ParsingState::derived`](crate::core::ParsingState::derived)).
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
    /// [`ImplementationError`](crate::core::constructs::ImplementationError) for a
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
    /// Deliberately infallible: this accessor only returns an already-configured
    /// resolver — failure belongs on [`SourceResolver::resolve`], which is already
    /// fallible and reports per reference. Embedding or binding code that cannot
    /// produce its resolver should report the failure through the embedding's own
    /// channel and answer `None` (the parse then resolves nothing).
    fn source_resolver(&self) -> Option<&dyn SourceResolver<L::SourceOrigin>> {
        None
    }

    // --- the group descent-delta channel ------------------------------------------

    /// The extra state change a group applies to its own interior, chosen from the
    /// group rule being entered.
    ///
    /// This is how entering a group of some class changes the conditions its content
    /// is parsed under. In the latexlike preset, a math-class rule returns
    /// `ParsingStateDelta::new().mode(Mode::Math)`, and the interior therefore parses
    /// in math mode. `None` — the default — applies no change beyond the standard
    /// descent derivation.
    ///
    /// **Must be a deterministic pure function of `(base, rule)`** — the result is
    /// memoized per `(base, rule)` by [`ParserSession::group_interior_state`] (`Arc`
    /// identities; the hook runs on memo **miss** only, so a call-count-dependent
    /// implementation would be observably wrong). The returned delta is merged with
    /// the descent invariant: the interior's
    /// [`expecting_group_close`](crate::core::token::TokenRules::expecting_group_close) is
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

    /// Builds the parser that runs at the root of one parse, over the whole source.
    ///
    /// Called once per parse: [`ParseSetup::parse`](super::ParseSetup::parse) runs
    /// the returned parser directly rather than as a descent, and freezes the session
    /// around the root node ([`BuildId`]) it returns. Ownership of the parser moves
    /// to the caller.
    ///
    /// The default is the standard [`RootNodesParser`] — the content loop over the
    /// whole source with stray-close recovery, staging the root `List`. Override it
    /// when every parse of the language needs a different root shape (scaffolding of
    /// the language's own around the content, a different root node); a single parse
    /// that does passes its parser through
    /// [`ParseSetup::with_root_parser`](super::ParseSetup::with_root_parser), which
    /// takes precedence over this factory. The contract a root parser must uphold —
    /// run at the top, not as a descent; the output is the tree's root; the
    /// pass-through delta is discarded — is documented on [`RootNodesParser`].
    ///
    /// # Errors
    ///
    /// `Err` means **the parser could not be built** and **aborts the parse** under
    /// any recovery policy, exactly as for
    /// [`make_nodes_parser`](ParseDriver::make_nodes_parser). An infallible
    /// implementation wraps its parser in `Ok(...)` and that is the only change.
    // The boxed-parser-or-abort pair is the decided factory signature; an alias
    // would only rename it.
    #[allow(clippy::type_complexity)]
    fn make_root_parser<'p>(
        &'p self,
    ) -> Result<Box<dyn ConstructParser<L, Output = BuildId> + 'p>, ParseError<L::SourceOrigin>>
    where
        L::InvocationSyntax: FromInvocation<L>,
    {
        Ok(Box::new(RootNodesParser::new()))
    }

    /// Builds the parser for one run of content: a group interior, an environment
    /// body, or the top level.
    ///
    /// Called once per such descent, through
    /// [`ParseContext::parse_nodes`](crate::core::constructs::ParseContext::parse_nodes).
    /// Every descent site goes through that wrapper, so one override applies to the
    /// whole parse; this is the supported way to install a dispatch loop of your own.
    /// Ownership of the parser moves to the caller.
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
    /// [`DescentLimitExceeded`](crate::core::constructs::DescentLimitExceeded) inside
    /// [`ParseContext::parse_construct`](crate::core::constructs::ParseContext::parse_construct),
    /// before any factory-built parser runs. Carry
    /// [`HookFailed`](crate::error::HookFailed) for an operational failure,
    /// [`ImplementationError`](crate::core::constructs::ImplementationError) for a
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

    /// Builds the parser for one group, given the `GroupOpen` token already consumed
    /// and the rule it matched.
    ///
    /// Called once per group descent, through
    /// [`ParseContext::parse_group`](crate::core::constructs::ParseContext::parse_group).
    ///
    /// The default is the standard [`GroupParser`], which derives the interior state
    /// through [`ParseContext::group_interior_state`](crate::core::constructs::ParseContext::group_interior_state) (where
    /// [`group_interior_delta`](ParseDriver::group_interior_delta) merges in) — prefer
    /// the delta channel for state-shaped customization; override this factory only
    /// for structurally different group parses.
    ///
    /// This factory is also where a language installs the **after-effect leak hook**
    /// ([`GroupParser::new_with_after_effects`], the `\gdef` shape): a group otherwise
    /// returns no after-effect, dropping its interior run's merged record with the
    /// descent. Installing it here rather than per descent site is what gives the
    /// language the hook at every group, which is what lets an escape compose outward
    /// through nested groups — see
    /// [`GroupAfterEffectsFn`](crate::core::constructs::GroupAfterEffectsFn).
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
        open: &Token<L>,
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

    /// Builds the parser for one invocation of a callable.
    ///
    /// Called once per invocation: the dispatch loops obtain every invocation parser
    /// through the driver. The default delegates to the resolved spec's own factory
    /// ([`CallableSpec::make_invocation_parser`] on `invocation.spec`), building no
    /// parser itself and calling no other driver method, so specs keep owning their
    /// invocation behavior. Overriding gives the driver one place to wrap or replace
    /// every invocation parser — for instrumentation, or to substitute a parser per
    /// language — which no per-spec override could provide.
    ///
    /// The caller has already consumed the trigger token whole; see
    /// [`StdInvocationParser`](crate::core::constructs::StdInvocationParser)'s
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
        token: &Token<L>,
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
        token: &Token<L>,
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

/// The standard scope-stack [`CommandResolver`] strategy.
///
/// Every command token resolves through [`resolve_command_in_scopes`] under the one
/// fixed [`command_type`](ScopesCommandResolver::command_type).
///
/// That field **is** the datum core cannot default — a language's command
/// [`CallableTypeId`](Lang::CallableTypeId) (contrast specials, where the provider
/// supplies the resolved type with the match); languages with several command-syntax
/// callable types write their own resolver — that is what the strategy point exists
/// for.
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
        token: &Token<L>,
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

/// The one ready-made [`ParseDriver`].
///
/// It stores the [`Recovery`] policy setting, a pluggable [`CommandResolver`]
/// strategy, and an optional [`SourceResolver`] — everything else keeps the trait
/// defaults.
///
/// It implements the trait for **every** language whose
/// [`SourceOrigin`](Lang::SourceOrigin) is `O` and whose commands `R` can resolve —
/// the [`TrivialLang`](crate::core::TrivialLang) default driver (`type Driver =
/// StdParseDriver`, all parameters defaulted).
///
/// **Which command resolver to reach for:** `()` resolves nothing — the
/// [`TrivialLang`](crate::core::TrivialLang)-style test-language pairing
/// (`StdParseDriver::new(Recovery::Strict, ())`), and right for languages without
/// command syntax; [`ScopesCommandResolver`]
/// resolves every command through the state's scope stack under one fixed callable
/// type — the standard shape for a command-bearing language; beyond those, implement
/// [`CommandResolver`] (or a whole [`ParseDriver`]) yourself.
///
/// # Why the two resolvers are stored differently
///
/// The command resolver is part of the language *definition*, fixed when
/// `type Driver = …` is written, and it is called on every command token. It is
/// therefore a generic parameter, `R`, resolved at compile time.
///
/// The source resolver is a capability of the embedding environment: it varies per
/// deployment, and it is consulted only once per `\input`-style inclusion, through
/// the type-erased [`ParseDriver::source_resolver`] accessor. It is therefore a
/// plain `Option<Arc<dyn SourceResolver<_>>>` field, which also spares callers a
/// placeholder type and `None`-inference noise when no resolver is configured.
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

// No tokenization bound: the ready-made driver takes its reader from the language's
// own `Lang::Tokenization` (the defaulted `make_token_reader`), so it serves every
// language, whatever its tokens and stream positions are.
impl<L: Lang, R: CommandResolver<L>> ParseDriver<L> for StdParseDriver<R, L::SourceOrigin> {
    fn recovery(&self) -> Recovery {
        self.recovery
    }

    /// Forwards to the driver's [`command_resolver`](StdParseDriver::command_resolver)
    /// strategy.
    fn resolve_command(
        &self,
        state: &ParsingState<L>,
        token: &Token<L>,
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

/// A successful command resolution — the payload of [`CommandResolution::Resolved`].
///
/// It names which invocation form the command resolved to, and the behavior spec to
/// drive its parse: exactly what the dispatch loop needs to build an
/// [`Invocation`](crate::core::constructs::Invocation) (the core cannot know a preset's
/// type ids).
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

/// The answer [`ParseDriver::resolve_command`] gives about one command token: it
/// resolved, it did not resolve, or resolution broke.
///
/// [`Resolved`](CommandResolution::Resolved) carries the invocation to dispatch. The
/// other two are failures, and each carries an optional `detail` string saying *why*,
/// which appears verbatim on the resulting diagnostic. Any resolution layer may fill
/// that in: the trait's default hook reports that command resolution is not
/// implemented at all, while a library-backed resolver might report where it searched
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
    /// ([`CommandResolutionFailed`](crate::core::constructs::CommandResolutionFailed)) so
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
/// the miss/failure detail policy are written once rather than duplicated per driver.
/// [`ScopesCommandResolver`] is its one-line packaging as the [`CommandResolver`]
/// strategy for [`StdParseDriver`].
///
/// Builds a [`CallableQuery`] with
/// [`CallableSyntax::Command`](crate::core::specs::CallableSyntax::Command) (the token's
/// fired escape character), consults
/// [`ScopeStack::retrieve_spec`](crate::core::specs::ScopeStack::retrieve_spec), and maps
/// the outcome: a hit is [`Resolved`](CommandResolution::Resolved); a clean miss is
/// [`Unresolved`](CommandResolution::Unresolved) carrying the searched providers —
/// and, where the scopes advertise their symbols, a **did-you-mean** hint — as
/// detail; an operational provider error is [`Failed`](CommandResolution::Failed)
/// carrying the provider's rendered error. The token is read through `tokens`, the
/// reader that produced it; a non-[`Command`](TokenKind::Command) token — a
/// caller-contract violation — yields `Unresolved { detail: None }`.
///
/// **The did-you-mean detail** scans the providers' advertised definitions
/// ([`SpecsProvider::iter_symbols`](crate::core::specs::SpecsProvider::iter_symbols),
/// under the queried callable type and the state's current mode) for near-misses
/// of the unresolved name: a definition registered *with* its escape character
/// (`\greet` instead of `greet` — the registration trap
/// [`Package::insert`](crate::core::specs::Package::insert)'s contract warns about) is
/// called out explicitly, and small-edit-distance names are suggested. Providers
/// that cannot enumerate are skipped, and an in-stack fallback provider makes
/// resolution *succeed*, so the miss path — hints included — never runs there
/// (accepted limitation; the parse-initialization check
/// `check_provider_commands_shadowed_by_escape` fires regardless of fallbacks).
pub fn resolve_command_in_scopes<L: Lang>(
    state: &ParsingState<L>,
    token: &Token<L>,
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
