//! Environments: the `\begin{name} … \end{name}` composition and its spec surface.
//!
//! The *notion* of "environment" is preset property; core contributes parameterized
//! building blocks only. This module promotes a composition once rehearsed test-side:
//!
//! - [`BeginSpec`] — the `\begin` dispatcher, registered as an ordinary
//!   [`Macro`](CallableType::Macro) entry of the [`base_package`](super::base_package)
//!   (decided at the 7.6 checkpoint: data in the scope stack, not driver code — it is
//!   shadowable and unloadable like any definition). Its invocation parser reads the
//!   rigid name group ([`read_rigid_name_group`]), resolves the environment's spec from
//!   the scope stack under [`CallableType::Environment`], parses declared arguments
//!   ([`parse_declared_arguments`] — frames quote the *environment's* name), drives the
//!   body parser, and stages the callable node with its `"body"` slot record.
//! - [`EndSpec`] — the orphan-`\end` diagnoser: inside an environment body, `\end` is
//!   the body parser's stop condition and never reaches command resolution, so a
//!   *resolved* `\end` is always an orphan.
//! - [`EnvironmentSpec`] — the registration type for environments: declared arguments
//!   plus body behavior, reachable through the sanctioned funnel pattern:
//!   the concrete wrapper holds an
//!   `Arc<dyn `[`EnvironmentBehavior`]`>`, whose defaulted methods carry the body
//!   state delta and the body-parser choice (pylatexenc's
//!   `EnvironmentSpec.make_body_parser` precedent; [`VerbatimBehavior`] overrides it
//!   for raw bodies).
//!
//! An environment spec's own
//! [`make_invocation_parser`](CallableSpec::make_invocation_parser) is never invoked —
//! the permanent boundary decided with the composition's rehoming: per-environment
//! variation flows through the [`EnvironmentBehavior`] surface. Starred environments
//! (`figure*`) are ordinary separate entries — `*` reads as a plain character inside
//! the rigid name group.
//!
//! ```
//! use techy::core::{Language, ParsingState};
//! use techy::error::Recovery;
//! use techy::latexlike::{CallableType, EnvironmentSpec, Latexlike, LatexlikeDriver};
//! use techy::core::specs::Package;
//!
//! let mut package = Package::new("mydefs");
//! package.insert(
//!     CallableType::Environment,
//!     "itemize",
//!     EnvironmentSpec::new(vec![]),
//! );
//! let language: Language<Latexlike> = Language::new(
//!     LatexlikeDriver::new(Recovery::Strict),
//!     ParsingState::lang_initial_with_packages([package]),
//! );
//!
//! let result = language.parse(r"\begin{itemize} a b \end{itemize}").unwrap();
//! let env = result.tree.root().child(0).unwrap();
//! assert_eq!(env.environment_name(), Some("itemize"));
//! assert_eq!(env.body().unwrap().len(), 1);
//! ```

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use alloc::format;

use crate::constructs::{
    parse_declared_arguments, read_rigid_name_group, ConstructParser,
    ConstructParserResult, EnvironmentBody, EnvironmentBodyParser,
    EnvironmentTerminatorFacts, Invocation, ParseContext, VerbatimBodyParser,
};
use crate::error::DiagnosticInfo;
use crate::node::{
    BodySlotExt, BuildId, CallableData, ChildRegion, NodeKind, ParsedArguments,
    ParsedSlot, ParsedSlots, SlotRole,
};
use core::marker::PhantomData;

use crate::scopes::{CallableQuery, CallableSyntax};
use crate::source::{SourceSpan, Span, TextContent};
use crate::spec::{ArgumentSpec, CallableSpec, FrameRole};
use crate::state::ParsingStateDelta;

use super::invocation_syntax::{EnvironmentSideSyntax, EnvironmentSyntax};
use super::lang::{
    LatexlikeCallableType, LatexlikeGroupType, LatexlikeInvocationSyntax, LatexlikeLang,
};
use super::spec::frame_title;
use super::Latexlike;

/// The command name that introduces every environment (`\begin`), under which
/// [`BeginSpec`] is registered in the [`base_package`](super::base_package).
pub(crate) const BEGIN_COMMAND_NAME: &str = "begin";

/// The terminator command name (`\end`): the body parser's stop condition, and the
/// [`base_package`](super::base_package) registration of [`EndSpec`].
pub(crate) const END_COMMAND_NAME: &str = "end";

// --- conditions --------------------------------------------------------------------

/// Condition: `\begin` was not followed immediately by its rigid name group
/// (`\begin [x]`, `\begin{ itemize }`). Tolerant recovery stages the trigger alone —
/// its syntactic post-space included, keeping the sibling partition exact — as a
/// `Chars` node (the accepted markup-in-chars recovery artifact) and consumes
/// nothing past it.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "latexlike.environments.malformed-begin",
    message = "malformed ‘\\begin’: expected the environment's name group immediately \
               after the command",
    no_constructor
)]
pub struct MalformedBegin;

/// Condition: `\begin{name}` named an environment no provider of the scope stack
/// defines. Tolerant recovery parses on with an argument-less body-only fallback
/// spec, so the body still runs to its `\end{name}` terminator.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "latexlike.environments.unknown-environment",
    message = "unknown environment ‘{name}’"
)]
pub struct UnknownEnvironment {
    /// The environment name as written in the `\begin` name group.
    pub name: String,
}

/// Condition: an `\end` with no environment open at its level. Inside a body the
/// terminator is consumed by the body parser before command resolution, so a
/// dispatched `\end` is always an orphan ([`EndSpec`]). Tolerant recovery stages the
/// consumed extent — `\end{name}` whole, or `\end` alone when the name group is
/// malformed — as a `Chars` node.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "latexlike.environments.orphan-end")]
pub struct OrphanEnd {
    /// The environment named by the terminator, when its name group parsed.
    pub name: Option<String>,
}

// Hand-written wording: the message quotes the name group only when it parsed (a
// match, which the message format string cannot express).
impl fmt::Display for OrphanEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(name) => {
                write!(f, "orphan ‘\\end{{{name}}}’: no matching ‘\\begin{{{name}}}’")
            }
            None => write!(f, "orphan ‘\\end’: no matching ‘\\begin’"),
        }
    }
}

// --- the environment spec surface --------------------------------------------------

/// The invocation facts of one environment being parsed — what
/// [`EnvironmentBehavior`]'s hooks receive from the `\begin` composition. Grows by
/// field as behavior hooks demand (`#[non_exhaustive]`); built only by the
/// composition.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct EnvironmentInvocation<'p> {
    /// The `\begin` trigger token's span — the anchor of body-level diagnostics
    /// (missing terminator).
    pub trigger_span: Span,
    /// The environment's name as written inside the name group (`itemize`,
    /// `figure*`).
    pub name: &'p str,
    /// The name's span (the name group's interior).
    pub name_span: Span,
    /// The begin trigger's escape character as written — the canonical escape a
    /// takeover body composes its terminator spelling from
    /// ([`VerbatimBehavior`]'s literal `\end{name}`).
    pub escape_char: char,
    /// The begin name group's open delimiter as written (off the matched rule).
    pub name_group_open: &'p str,
    /// The begin name group's close delimiter as written.
    pub name_group_close: &'p str,
}

/// The behavior of one environment, behind [`EnvironmentSpec`] — the funnel's inner
/// trait: third-party implementations override the
/// defaulted methods; the composition reaches them through the concrete wrapper's
/// downcast. The pylatexenc `EnvironmentSpec` analog (`make_body_parser`,
/// `make_body_parsing_state_delta`), with the declarative standard implementation
/// behind [`EnvironmentSpec::new`].
pub trait EnvironmentBehavior<LLL: LatexlikeLang = Latexlike>: fmt::Debug + Send + Sync {
    /// The declarative argument structure of the environment, in invocation order —
    /// parsed right after the `\begin{name}` scaffolding, before the body. Default:
    /// no arguments.
    fn arguments(&self) -> &[Arc<ArgumentSpec<LLL>>] {
        &[]
    }

    /// The body's parsing-state delta (mode changes, tokenization tweaks), stacked on
    /// the invocation's base state for the body's whole extent — terminator included —
    /// and reverted structurally after (pylatexenc's `make_body_parsing_state_delta`).
    /// Default: none.
    fn body_state_delta(
        &self,
        invocation: EnvironmentInvocation<'_>,
    ) -> Option<ParsingStateDelta<LLL>> {
        let _ = invocation;
        None
    }

    /// The parser that reads the environment's body, entered right after the declared
    /// arguments under the body state. Default: the core [`EnvironmentBodyParser`] —
    /// content up to and including the rigid `\end{name}` terminator, staged as one
    /// body `List`. Override for takeover bodies ([`VerbatimBehavior`]'s raw read);
    /// the returned parser must produce an [`EnvironmentBody`] and no pass-through
    /// delta (its reported [terminator facts](EnvironmentBody::terminator) feed the
    /// invocation-syntax recording).
    fn make_body_parser<'p>(
        &'p self,
        invocation: EnvironmentInvocation<'p>,
    ) -> Box<dyn ConstructParser<LLL, Output = EnvironmentBody<LLL>> + 'p> {
        default_body_parser(invocation)
    }
}

/// The default body of [`EnvironmentBehavior::make_body_parser`], shared with the
/// composition's non-[`EnvironmentSpec`] fallback: the core [`EnvironmentBodyParser`]
/// over the preset's terminator shape.
fn default_body_parser<'p, LLL: LatexlikeLang>(
    invocation: EnvironmentInvocation<'p>,
) -> Box<dyn ConstructParser<LLL, Output = EnvironmentBody<LLL>> + 'p> {
    Box::new(
        EnvironmentBodyParser::new(
            invocation.trigger_span,
            invocation.name,
            END_COMMAND_NAME,
            LLL::GroupTypeId::content_group(),
        )
        .with_invocation_name_span(invocation.name_span),
    )
}

/// The declarative standard [`EnvironmentBehavior`] behind [`EnvironmentSpec::new`]:
/// arguments as plain data, body handling per the trait defaults.
struct StdEnvironmentBehavior<LLL: LatexlikeLang> {
    arguments: Vec<Arc<ArgumentSpec<LLL>>>,
}

impl<LLL: LatexlikeLang> EnvironmentBehavior<LLL> for StdEnvironmentBehavior<LLL> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<LLL>>] {
        &self.arguments
    }
}

impl<LLL: LatexlikeLang> fmt::Debug for StdEnvironmentBehavior<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StdEnvironmentBehavior")
            .field("arguments", &self.arguments)
            .finish()
    }
}

/// The verbatim-environment behavior (pylatexenc's
/// `LatexVerbatimEnvironmentContentsParser` wired as an [`EnvironmentBehavior`]):
/// declared arguments parse normally — tokenized, before the raw region begins
/// (`lstlisting`-style options) — and the **body is raw text**, read by the core
/// [`VerbatimBodyParser`] up to the literal `\end{name}` terminator (composed per
/// invocation with the preset's canonical `\` escape and `{…}` name group, the same
/// spellings the `\begin` composition itself is built on). The single newline right
/// after the begin scaffolding is staged but designated out of the body content
/// (the gobble rule — see [`VerbatimBodyParser`]).
///
/// One behavior instance serves any environment name (the terminator back-reference
/// comes from the invocation), so `verbatim`, `verbatim*`, and listing-style
/// environments can share it:
///
/// ```
/// use std::sync::Arc;
/// use techy::core::{Language, ParsingState};
/// use techy::error::Recovery;
/// use techy::latexlike::{
///     CallableType, EnvironmentSpec, Latexlike, LatexlikeDriver, VerbatimBehavior,
/// };
/// use techy::core::specs::Package;
///
/// let mut package = Package::new("mydefs");
/// package.insert(
///     CallableType::Environment,
///     "verbatim",
///     EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::default())),
/// );
/// let language: Language<Latexlike> = Language::new(
///     LatexlikeDriver::new(Recovery::Strict),
///     ParsingState::lang_initial_with_packages([package]),
/// );
///
/// let result = language
///     .parse("\\begin{verbatim}\na % b \\x{\n\\end{verbatim}")
///     .unwrap();
/// let env = result.tree.root().child(0).unwrap();
/// assert_eq!(env.environment_name(), Some("verbatim"));
/// let body: Vec<_> = env.body().unwrap().iter().collect();
/// assert_eq!(body.len(), 1);
/// assert_eq!(body[0].chars(), Some("a % b \\x{\n"));
/// ```
pub struct VerbatimBehavior<LLL: LatexlikeLang = Latexlike> {
    arguments: Vec<Arc<ArgumentSpec<LLL>>>,
}

impl<LLL: LatexlikeLang> VerbatimBehavior<LLL> {
    /// A verbatim-body environment with the given (ordinarily parsed) argument
    /// structure ahead of the raw body.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<LLL>>>) -> VerbatimBehavior<LLL> {
        VerbatimBehavior { arguments }
    }
}

impl<LLL: LatexlikeLang> EnvironmentBehavior<LLL> for VerbatimBehavior<LLL> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<LLL>>] {
        &self.arguments
    }

    fn make_body_parser<'p>(
        &'p self,
        invocation: EnvironmentInvocation<'p>,
    ) -> Box<dyn ConstructParser<LLL, Output = EnvironmentBody<LLL>> + 'p> {
        // The literal terminator, composed from the invocation's own spellings
        // (the begin trigger's escape char and the begin name group's
        // delimiters) — the same bytes the standard end facts are recorded from.
        Box::new(
            VerbatimBodyParser::new(
                invocation.trigger_span,
                invocation.name,
                format!(
                    "{}{}{}{}{}",
                    invocation.escape_char,
                    END_COMMAND_NAME,
                    invocation.name_group_open,
                    invocation.name,
                    invocation.name_group_close,
                ),
                LLL::GroupTypeId::verbatim_group(),
            )
            .with_invocation_name_span(invocation.name_span),
        )
    }
}

impl<LLL: LatexlikeLang> Default for VerbatimBehavior<LLL> {
    fn default() -> Self {
        VerbatimBehavior { arguments: Vec::new() }
    }
}

impl<LLL: LatexlikeLang> fmt::Debug for VerbatimBehavior<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerbatimBehavior").field("arguments", &self.arguments).finish()
    }
}

/// [`EnvironmentSpec::with_body_delta`]'s adapter: reports the given body state
/// delta — overriding whatever the wrapped behavior computes — and delegates
/// everything else.
struct BodyDeltaOverride<LLL: LatexlikeLang> {
    inner: Arc<dyn EnvironmentBehavior<LLL>>,
    delta: ParsingStateDelta<LLL>,
}

impl<LLL: LatexlikeLang> EnvironmentBehavior<LLL> for BodyDeltaOverride<LLL> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<LLL>>] {
        self.inner.arguments()
    }

    fn body_state_delta(
        &self,
        _invocation: EnvironmentInvocation<'_>,
    ) -> Option<ParsingStateDelta<LLL>> {
        Some(self.delta.clone())
    }

    fn make_body_parser<'p>(
        &'p self,
        invocation: EnvironmentInvocation<'p>,
    ) -> Box<dyn ConstructParser<LLL, Output = EnvironmentBody<LLL>> + 'p> {
        self.inner.make_body_parser(invocation)
    }
}

impl<LLL: LatexlikeLang> fmt::Debug for BodyDeltaOverride<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BodyDeltaOverride")
            .field("inner", &self.inner)
            .field("delta", &self.delta)
            .finish()
    }
}

/// The preset's environment spec: the registration type for
/// [`CallableType::Environment`](super::CallableType::Environment) entries — the funnel wrapper through which the `\begin` composition reaches the environment's
/// [`EnvironmentBehavior`] (`Any` downcasts hit concrete types only, so the open set
/// of behaviors funnels through this one concrete spec type).
///
/// The composition — not this spec — parses the invocation:
/// [`make_invocation_parser`](CallableSpec::make_invocation_parser) is never
/// consulted for environment entries (the decided permanent boundary; registering an
/// `EnvironmentSpec` under another callable type gets the macro-shaped default
/// parse, which reads the arguments and no body). A generic non-`EnvironmentSpec`
/// [`CallableSpec`] under [`CallableType::Environment`](super::CallableType::Environment) is legitimate too: its
/// declared arguments parse and the body takes the default handling.
pub struct EnvironmentSpec<LLL: LatexlikeLang = Latexlike> {
    behavior: Arc<dyn EnvironmentBehavior<LLL>>,
}

impl<LLL: LatexlikeLang> EnvironmentSpec<LLL> {
    /// A declarative environment with the given argument structure and the default
    /// body handling.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<LLL>>>) -> EnvironmentSpec<LLL> {
        EnvironmentSpec::from_behavior(Arc::new(StdEnvironmentBehavior { arguments }))
    }

    /// An environment driven by a custom [`EnvironmentBehavior`] — the funnel's
    /// registration entry for behavior-shaped customization (verbatim-like bodies).
    pub fn from_behavior(behavior: Arc<dyn EnvironmentBehavior<LLL>>) -> EnvironmentSpec<LLL> {
        EnvironmentSpec { behavior }
    }

    /// Set the body's parsing-state delta (`equation` entering
    /// [`Mode::Math`](super::Mode), a listing disabling comment tokenization),
    /// overriding whatever the current behavior computes.
    pub fn with_body_delta(self, delta: ParsingStateDelta<LLL>) -> EnvironmentSpec<LLL> {
        EnvironmentSpec {
            behavior: Arc::new(BodyDeltaOverride { inner: self.behavior, delta }),
        }
    }

    /// The behavior driving this environment's parse.
    pub fn behavior(&self) -> &dyn EnvironmentBehavior<LLL> {
        &*self.behavior
    }
}

impl<LLL: LatexlikeLang> CallableSpec<LLL> for EnvironmentSpec<LLL> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<LLL>>] {
        self.behavior.arguments()
    }

    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        frame_title("environment", role, name)
    }
}

impl<LLL: LatexlikeLang> Clone for EnvironmentSpec<LLL> {
    fn clone(&self) -> Self {
        EnvironmentSpec { behavior: Arc::clone(&self.behavior) }
    }
}

impl<LLL: LatexlikeLang> fmt::Debug for EnvironmentSpec<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnvironmentSpec").field("behavior", &self.behavior).finish()
    }
}

// --- the `\begin` dispatcher and its composition ------------------------------------

/// The `\begin` dispatcher: every environment enters through this shared spec, an
/// ordinary [`Macro`](super::CallableType::Macro) entry of the
/// [`base_package`](super::base_package) (shadowable and unloadable like any
/// definition). Its parser is the preset's environment composition (module docs).
pub struct BeginSpec<LLL: LatexlikeLang = Latexlike> {
    lang: PhantomData<fn() -> LLL>,
}

impl<LLL: LatexlikeLang> BeginSpec<LLL> {
    /// The `\begin` dispatcher spec.
    pub fn new() -> BeginSpec<LLL> {
        BeginSpec { lang: PhantomData }
    }
}

// The `SlotExt: BodySlotExt` clause is the body-marking contract: the composition
// mints the environment's body slot ext through the generic `BodySlotExt`
// mechanism, so `\begin` is registrable exactly for family members whose slot ext
// implements it (`()` and the preset's `BodyMarker` both do).
impl<LLL: LatexlikeLang> CallableSpec<LLL> for BeginSpec<LLL>
where
    crate::node::SlotExt<LLL>: BodySlotExt,
{
    /// `\begin` declares nothing but reads an entire environment: bare use as a
    /// single-token expression argument is diagnosed, not dispatched — a deliberate,
    /// documented divergence from pylatexenc, which dispatches the environment as the
    /// argument.
    fn requires_content(&self) -> bool {
        true
    }

    fn make_invocation_parser<'a, 's>(
        &'a self,
        invocation: Invocation<'a, 's, LLL>,
    ) -> Box<dyn ConstructParser<LLL, Output = BuildId> + 'a>
    where
        's: 'a,
    {
        Box::new(EnvironmentInvocationParser { invocation })
    }

    /// `\begin` itself is macro-shaped; the environment's own name titles the frames
    /// pushed once the composition has read it.
    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        frame_title("macro", role, name)
    }
}

impl<LLL: LatexlikeLang> Default for BeginSpec<LLL> {
    fn default() -> Self {
        BeginSpec::new()
    }
}

impl<LLL: LatexlikeLang> Clone for BeginSpec<LLL> {
    fn clone(&self) -> Self {
        BeginSpec::new()
    }
}

impl<LLL: LatexlikeLang> Copy for BeginSpec<LLL> {}

impl<LLL: LatexlikeLang> fmt::Debug for BeginSpec<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BeginSpec").finish()
    }
}

/// The orphan-`\end` spec: a resolved `\end` never belongs to an environment (the
/// body parser consumes well-formed terminators before command resolution), so its
/// parser diagnoses [`OrphanEnd`] and recovers. An ordinary
/// [`Macro`](super::CallableType::Macro) entry of the [`base_package`](super::base_package),
/// alongside [`BeginSpec`].
pub struct EndSpec<LLL: LatexlikeLang = Latexlike> {
    lang: PhantomData<fn() -> LLL>,
}

impl<LLL: LatexlikeLang> EndSpec<LLL> {
    /// The orphan-`\end` diagnoser spec.
    pub fn new() -> EndSpec<LLL> {
        EndSpec { lang: PhantomData }
    }
}

impl<LLL: LatexlikeLang> CallableSpec<LLL> for EndSpec<LLL> {
    /// Like `\begin`: declares nothing, reads material (its name group) — bare
    /// expression use is diagnosed.
    fn requires_content(&self) -> bool {
        true
    }

    fn make_invocation_parser<'a, 's>(
        &'a self,
        invocation: Invocation<'a, 's, LLL>,
    ) -> Box<dyn ConstructParser<LLL, Output = BuildId> + 'a>
    where
        's: 'a,
    {
        Box::new(OrphanEndParser { invocation })
    }

    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        frame_title("macro", role, name)
    }
}

impl<LLL: LatexlikeLang> Default for EndSpec<LLL> {
    fn default() -> Self {
        EndSpec::new()
    }
}

impl<LLL: LatexlikeLang> Clone for EndSpec<LLL> {
    fn clone(&self) -> Self {
        EndSpec::new()
    }
}

impl<LLL: LatexlikeLang> Copy for EndSpec<LLL> {}

impl<LLL: LatexlikeLang> fmt::Debug for EndSpec<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EndSpec").finish()
    }
}

/// The environment composition (a tier-2 temporary, [`BeginSpec`]'s parser):
/// scaffolding, resolution, arguments, body, node assembly — minimal scanning code of
/// its own, assembled from the public core building blocks (module docs).
struct EnvironmentInvocationParser<'a, 's, LLL: LatexlikeLang> {
    invocation: Invocation<'a, 's, LLL>,
}

impl<LLL: LatexlikeLang> ConstructParser<LLL> for EnvironmentInvocationParser<'_, '_, LLL>
where
    crate::node::SlotExt<LLL>: BodySlotExt,
{
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, LLL>,
    ) -> ConstructParserResult<LLL, (BuildId, Option<ParsingStateDelta<LLL>>)>
    {
        // The language's environment-side record ([`LatexlikeInvocationSyntax::Env`])
        // owns the begin/end syntax; the composition owns resolution, arguments,
        // and node assembly.
        type Env<LLL> =
            <<LLL as crate::state::Lang>::InvocationSyntax as LatexlikeInvocationSyntax<
                LLL,
            >>::Env;
        let trigger = self.invocation.token;

        // Begin-side scaffolding scan, delegated to the environment-syntax record
        // ([`EnvironmentSyntax::parse_begin`]): the rigid name group must be the
        // immediately next token; the begin-side spelling facts (escape char,
        // command word, post-space, name-group rule) are recorded on the
        // accumulator, no longer normalized away.
        let Some((name_group, mut env_syntax)) = Env::<LLL>::parse_begin(cx, trigger)?
        else {
            cx.recover(MalformedBegin, SourceSpan::new(&cx.source, trigger.span))?;
            // Chars fallback over the trigger alone (markup in a Chars node is the
            // accepted tolerant-recovery artifact); nothing past it is consumed.
            let id = cx.stage_node(
                    NodeKind::chars(trigger.span),
                    SourceSpan::new(&cx.source, trigger.span),
                    Arc::clone(&cx.state),
                    vec![],
                )
                .map_err(|error| cx.implementation_error(error, trigger.span))?;
            return Ok((id, None));
        };
        let source = Arc::clone(&cx.source);
        let name = &source.content()[name_group.name_span.range()];

        // Resolve the environment's spec by name through the scope stack. A provider
        // failure is an operational error, not a source condition — abort via the
        // implementation-error path.
        let query = CallableQuery::new(
            LLL::CallableTypeId::environment_callable(),
            name,
            CallableSyntax::Other,
        );
        let resolved = cx
            .state
            .scopes()
            .retrieve_spec(&query, &cx.state)
            .map_err(|error| cx.implementation_error(error, name_group.name_span))?;
        let spec: Arc<dyn CallableSpec<LLL>> = match resolved {
            Some(spec) => spec,
            None => {
                cx.recover(
                    UnknownEnvironment::new(name),
                    SourceSpan::new(&cx.source, name_group.name_span),
                )?;
                // Tolerant fallback: an argument-less body-only environment, so the
                // body still parses to its terminator.
                Arc::new(EnvironmentSpec::<LLL>::new(vec![]))
            }
        };

        // Declared arguments: the shared core loop; the argument frames quote the
        // *environment's* name, not `\begin`.
        let (mut children, arguments) =
            parse_declared_arguments(cx, &spec, name_group.name_span)?;

        // The environment's behavior, through the funnel downcast. A
        // non-`EnvironmentSpec` registration has no behavior to offer and gets the
        // default body handling.
        let behavior: Option<&dyn EnvironmentBehavior<LLL>> = (&*spec
            as &dyn core::any::Any)
            .downcast_ref::<EnvironmentSpec<LLL>>()
            .map(|environment_spec| environment_spec.behavior());
        // The invocation facts handed to the behavior hooks, spelling pieces
        // included (a takeover body composes its terminator from them). The rule
        // `Arc` clone pins the delimiter strings for the borrow.
        let name_group_rule = Arc::clone(&name_group.rule);
        let env_invocation = EnvironmentInvocation {
            trigger_span: trigger.span,
            name,
            name_span: name_group.name_span,
            escape_char: match &trigger.kind {
                crate::token::TokenKind::Command { escape_char, .. } => *escape_char,
                _ => '\u{0}',
            },
            name_group_open: &name_group_rule.open,
            name_group_close: &name_group_rule.close,
        };

        // The body: parsed under the behavior's state delta stacked on the
        // invocation's base (session-mediated, structurally reverted), by the
        // behavior's body parser — content up to and including the `\end{name}`
        // terminator in the default shape.
        let body_delta = behavior.and_then(|b| b.body_state_delta(env_invocation));
        let body_state = match &body_delta {
            Some(delta) => cx.derive_state(delta)?,
            None => Arc::clone(&cx.state),
        };
        let mut body_parser = match behavior {
            Some(b) => b.make_body_parser(env_invocation),
            None => default_body_parser(env_invocation),
        };
        let (body, passthrough) = cx.parse_scoped(body_state, &mut *body_parser)?;
        drop(body_parser);
        debug_assert!(passthrough.is_none(), "the body parser returns no pass-through delta");

        // End-side facts, reported back by the body parser (the terminator
        // consumer): a tokenized terminator fills the end side verbatim; a raw
        // (verbatim) body consumed its terminator as one literal token, so the
        // record notes standard-shaped end facts; a body that closed without a
        // terminator (mismatch, malformed, end of input) leaves the end side
        // empty.
        match &body.terminator {
            Some(EnvironmentTerminatorFacts::Scanned {
                escape_char,
                command_word,
                post_space,
                name_group,
            }) => env_syntax.parse_end(EnvironmentSideSyntax {
                escape_char: *escape_char,
                command_word: TextContent::Spanned(*command_word),
                post_space: TextContent::Spanned(*post_space),
                name_group_rule: Arc::clone(&name_group.rule),
            }),
            Some(EnvironmentTerminatorFacts::Literal { .. }) => {
                env_syntax.record_std_end_facts(END_COMMAND_NAME);
            }
            None => {}
        }

        let offset = children.len() as u32;
        children.push(body.body);
        // The slot record is pure node vocabulary: minted here, name carried on the
        // record itself; the content designation is the body parser's
        // (`EnvironmentBody::content` — a verbatim body designates its gobbled
        // newline out, Phase 7.7). The ext is minted through `BodySlotExt` (not a
        // concrete marker type), so machinery generic over the language keeps working.
        let slots = ParsedSlots::from(vec![ParsedSlot::new(
            ChildRegion::new(offset..offset + 1, body.content),
            "body",
            SlotRole::Content,
            BodySlotExt::make_body(),
        )]);

        let data = CallableData {
            // The environment's own identity — not the `\begin` macro's.
            callable_type: LLL::CallableTypeId::environment_callable(),
            name: name.into(),
            spec,
            arguments: ParsedArguments::from(arguments),
            slots,
            // The recorded begin/end scaffolding facts — the environment arm of
            // the Lang-owned invocation-syntax payload (whitespace after the
            // begin/end commands is a recorded fact now, not a normalized-away
            // gap; whitespace after `\end{…}` stays sibling content).
            invocation_syntax: LLL::InvocationSyntax::environment_form(env_syntax),
        };
        let id = cx.stage_node(
                NodeKind::callable(data),
                SourceSpan::new(&cx.source, trigger.span.start()..body.end),
                Arc::clone(&cx.state),
                children,
            )
            .map_err(|error| {
                cx.implementation_error(error, Span::new(trigger.span.start(), body.end))
            })?;
        Ok((id, None))
    }
}

/// The orphan-`\end` recovery parser ([`EndSpec`]'s): read the name group when
/// present, diagnose [`OrphanEnd`], stage the consumed extent as a `Chars` node.
struct OrphanEndParser<'a, 's, LLL: LatexlikeLang> {
    invocation: Invocation<'a, 's, LLL>,
}

impl<LLL: LatexlikeLang> ConstructParser<LLL> for OrphanEndParser<'_, '_, LLL> {
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, LLL>,
    ) -> ConstructParserResult<LLL, (BuildId, Option<ParsingStateDelta<LLL>>)>
    {
        let trigger = self.invocation.token;
        let source = Arc::clone(&cx.source);
        let (name, end) =
            match read_rigid_name_group(cx, LLL::GroupTypeId::content_group())? {
            Some(group) => (
                Some(String::from(&source.content()[group.name_span.range()])),
                group.end,
            ),
            // Malformed name group: nothing past the trigger was consumed.
            None => (None, trigger.span.end()),
        };
        let span = Span::new(trigger.span.start(), end);
        cx.recover(OrphanEnd::new(name), SourceSpan::new(&cx.source, span))?;
        let id = cx.stage_node(
                NodeKind::chars(span),
                SourceSpan::new(&cx.source, span),
                Arc::clone(&cx.state),
                vec![],
            )
            .map_err(|error| cx.implementation_error(error, span))?;
        Ok((id, None))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::root_shapes;
    use super::super::{CallableType, GroupType, LatexlikeDriver, MacroSpec, Mode};
    use super::*;
    use crate::constructs::{
        ExpressionParser, GroupArgumentParser, OptionalGroupArgumentParser,
    };
    use crate::engine::{Language, ParseResult};
    use crate::error::Recovery;
    use crate::node::{check_tree_invariants, NodeRef};
    use crate::scopes::{Package, ScopeOp};
    use crate::state::{ParsingState, TokenRulesOverrides};
    use crate::token::GroupRule;
    use alloc::string::ToString;

    // --- the suite's definitions: the §G environment matrix over the real preset ------

    fn brace_arg() -> Arc<ArgumentSpec<Latexlike>> {
        Arc::new(ArgumentSpec::new_unnamed(Arc::new(GroupArgumentParser::new(GroupType::Content))))
    }

    fn optional_arg() -> Arc<ArgumentSpec<Latexlike>> {
        Arc::new(ArgumentSpec::new_unnamed(Arc::new(OptionalGroupArgumentParser::new(Arc::new(
            GroupRule { group_type: GroupType::Content, open: "[".into(), close: "]".into() },
        )))))
    }

    fn expr_arg() -> Arc<ArgumentSpec<Latexlike>> {
        Arc::new(ArgumentSpec::new_unnamed(Arc::new(ExpressionParser::new())))
    }

    fn env(arguments: Vec<Arc<ArgumentSpec<Latexlike>>>) -> Arc<EnvironmentSpec> {
        Arc::new(EnvironmentSpec::new(arguments))
    }

    fn test_definitions() -> Package<Latexlike> {
        let mut package = Package::new("test-definitions");
        package.insert(CallableType::Environment, "itemize", env(vec![]));
        package.insert(CallableType::Environment, "tabular", env(vec![brace_arg()]));
        package.insert(CallableType::Environment, "figure", env(vec![optional_arg()]));
        // Starred names are ordinary separate entries.
        package.insert(CallableType::Environment, "figure*", env(vec![optional_arg()]));
        package.insert(
            CallableType::Environment,
            "nocomments",
            Arc::new(EnvironmentSpec::new(vec![]).with_body_delta(
                ParsingStateDelta::new().rules(TokenRulesOverrides {
                    enable_comments: Some(false),
                    ..TokenRulesOverrides::default()
                }),
            )),
        );
        package.insert(
            CallableType::Environment,
            "equation",
            Arc::new(
                EnvironmentSpec::new(vec![])
                    .with_body_delta(ParsingStateDelta::new().mode(Mode::Math)),
            ),
        );
        package.insert(CallableType::Environment, "A", env(vec![]));
        package.insert(CallableType::Environment, "B", env(vec![]));
        package.insert(
            CallableType::Macro,
            "frac",
            Arc::new(MacroSpec::new(vec![expr_arg(), expr_arg()])),
        );
        // Verbatim bodies (7.7): the plain environment, a starred sibling entry, and
        // a listing-style one with a tokenized option argument before the raw body.
        let verbatim = || {
            Arc::new(EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::default())))
        };
        package.insert(CallableType::Environment, "verbatim", verbatim());
        package.insert(CallableType::Environment, "verbatim*", verbatim());
        package.insert(
            CallableType::Environment,
            "lstlisting",
            Arc::new(EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::new(vec![
                optional_arg(),
            ])))),
        );
        package
    }

    fn language(recovery: Recovery) -> Language<Latexlike> {
        Language::new(
            LatexlikeDriver::new(recovery),
            ParsingState::lang_initial_with_packages([test_definitions()]),
        )
    }

    fn strict() -> Language<Latexlike> {
        language(Recovery::Strict)
    }

    fn tolerant() -> Language<Latexlike> {
        language(Recovery::Tolerant)
    }

    /// Strict parse expected clean: invariants checked, no diagnostics.
    fn parse_ok(input: &str) -> ParseResult<Latexlike> {
        let result = strict().parse(input).unwrap();
        check_tree_invariants(&result.tree);
        assert!(
            result.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            result.diagnostics
        );
        result
    }

    /// Tolerant parse: invariants checked, diagnostics left to the caller.
    fn parse_tolerant(input: &str) -> ParseResult<Latexlike> {
        let result = tolerant().parse(input).unwrap();
        check_tree_invariants(&result.tree);
        result
    }

    fn body_shapes(env: NodeRef<'_, Latexlike>) -> Vec<String> {
        env.body().expect("an environment node").iter().map(|node| node.summary()).collect()
    }

    fn messages(result: &ParseResult<Latexlike>) -> Vec<String> {
        result.diagnostics.iter().map(|d| d.message().to_string()).collect()
    }

    // --- the environment shape ---------------------------------------------------------

    #[test]
    fn environment_round_trips_with_exact_spans() {
        //             0....5....1....1....2....2....3....3
        //                  0    5    0    5    0    5
        let content = "x \\begin{itemize} a b \\end{itemize} y";
        let result = parse_ok(content);

        assert_eq!(
            root_shapes(&result),
            ["chars(x )", "Environment(itemize)", "chars( y)"]
        );
        let env = result.tree.root().child(1).unwrap();
        assert_eq!(env.span().range(), 2..35);
        assert_eq!(env.callable_type(), Some(CallableType::Environment));
        assert_eq!(env.environment_name(), Some("itemize"));
        // Environment shapes record empty post-space; the space after `\end{itemize}`
        // is sibling content.
        assert_eq!(env.post_space(), Some(""));
        // Children = the body `List` alone; its span is the body's content interior.
        assert_eq!(env.child_count(), 1);
        let body = env.slot_content_parent(0).expect("body list");
        assert!(body.is_list());
        assert_eq!(body.span().range(), 17..22);
        assert_eq!(body_shapes(env), ["chars( a b )"]);
        assert_eq!(env.arguments().unwrap().len(), 0);
        assert_eq!(env.slots().unwrap().len(), 1);
        assert!(env.slots().unwrap().get_named("body").is_some());
    }

    /// The preset's body slot is minted with `SlotRole::Content` and the
    /// `BodySlotExt` body marker — `body()` finds it through the marker, not
    /// through a slot position.
    #[test]
    fn body_slot_carries_content_role_and_body_marker() {
        let result = parse_ok("\\begin{itemize} a \\end{itemize}");
        let env = result.tree.root().child(0).unwrap();
        let slot = env.slots().unwrap().get_named("body").expect("the body slot");
        assert_eq!(slot.role, SlotRole::Content);
        assert!(slot.ext.is_body());
        assert_eq!(body_shapes(env), ["chars( a )"]);
    }

    #[test]
    fn empty_body_is_an_empty_list() {
        let result = parse_ok("\\begin{itemize}\\end{itemize}");
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.span().range(), 0..28);
        let body = env.slot_content_parent(0).unwrap();
        assert_eq!(body.span().range(), 15..15);
        assert_eq!(env.body().unwrap().iter().count(), 0);
    }

    #[test]
    fn scaffolding_whitespace_is_tolerated_and_unrecorded() {
        let result = parse_ok("\\begin {itemize}x\\end {itemize}");
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.span().range(), 0..31);
        assert_eq!(env.environment_name(), Some("itemize"));
        assert_eq!(env.post_space(), Some(""));
        assert_eq!(body_shapes(env), ["chars(x)"]);
    }

    #[test]
    fn declared_arguments_parse_before_the_body() {
        let result = parse_ok("\\begin{tabular}{cc}x\\end{tabular}");
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.environment_name(), Some("tabular"));
        // Children: the argument group + the body list.
        assert_eq!(env.child_count(), 2);
        let arguments = env.arguments().unwrap();
        assert_eq!(arguments.len(), 1);
        assert!(arguments.get(0).unwrap().is_provided());
        assert_eq!(body_shapes(env), ["chars(x)"]);
    }

    #[test]
    fn optional_arguments_and_starred_names() {
        let with_opt = parse_ok("\\begin{figure}[t]x\\end{figure}");
        let env = with_opt.tree.root().child(0).unwrap();
        assert!(env.arguments().unwrap().get(0).unwrap().is_provided());

        let without_opt = parse_ok("\\begin{figure}x\\end{figure}");
        let env = without_opt.tree.root().child(0).unwrap();
        assert!(!env.arguments().unwrap().get(0).unwrap().is_provided());

        // `figure*` is its own entry: `*` reads as a plain name character.
        let starred = parse_ok("\\begin{figure*}[t]x\\end{figure*}");
        let env = starred.tree.root().child(0).unwrap();
        assert_eq!(env.environment_name(), Some("figure*"));
        assert!(env.arguments().unwrap().get(0).unwrap().is_provided());
    }

    #[test]
    fn nested_environments() {
        let result = parse_ok("\\begin{A}x\\begin{B}y\\end{B}z\\end{A}");
        let outer = result.tree.root().child(0).unwrap();
        assert_eq!(outer.environment_name(), Some("A"));
        assert_eq!(
            body_shapes(outer),
            ["chars(x)", "Environment(B)", "chars(z)"]
        );
        let inner = outer.body().unwrap().iter().nth(1).unwrap();
        assert_eq!(body_shapes(inner), ["chars(y)"]);
    }

    #[test]
    fn environments_and_math_compose() {
        // A math group inside a body, and an environment inside a math group.
        let math_in_body = parse_ok("\\begin{itemize}$x$\\end{itemize}");
        let env = math_in_body.tree.root().child(0).unwrap();
        assert_eq!(body_shapes(env), ["group(Math(Inline) $ $)"]);

        let env_in_math = parse_ok("$\\begin{itemize}x\\end{itemize}$");
        let math = env_in_math.tree.root().child(0).unwrap();
        assert!(math.is_math_group());
        assert_eq!(math.child(0).unwrap().summary(), "Environment(itemize)");
    }

    // --- body state deltas -------------------------------------------------------------

    #[test]
    fn body_delta_disables_comment_tokenization() {
        // `nocomments` turns comments off for its body: `%` stays a plain character.
        let result = parse_ok("\\begin{nocomments}a%b\\end{nocomments}");
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(body_shapes(env), ["chars(a%b)"]);

        // The same content in a plain environment parses a comment node.
        let plain = parse_ok("\\begin{itemize}a%b\n\\end{itemize}");
        let env = plain.tree.root().child(0).unwrap();
        assert_eq!(body_shapes(env), ["chars(a)", "comment(b)"]);
    }

    #[test]
    fn equation_body_parses_in_math_mode() {
        let result = parse_ok("y\\begin{equation}x\\end{equation}z");
        let env = result.tree.root().child(1).unwrap();
        let interior = env.body().unwrap().iter().next().unwrap();
        assert_eq!(interior.chars(), Some("x"));
        assert_eq!(interior.parsing_state().mode(), Mode::Math);
        // The environment node itself and the following content are text-mode
        // (structural reversion).
        assert_eq!(env.parsing_state().mode(), Mode::Text);
        let after = result.tree.root().child(2).unwrap();
        assert_eq!(after.parsing_state().mode(), Mode::Text);
    }

    #[test]
    fn environment_visibility_gates_by_mode() {
        // A math-only package defining `aligned`: resolvable inside `$…$`, unknown in
        // text mode.
        let with_math_envs = |recovery| {
            let mut math_envs = Package::new("mathenvs");
            math_envs.insert(CallableType::Environment, "aligned", env(vec![]));
            math_envs.set_visible_modes(Some(vec![Mode::Math]));
            Language::new(
                LatexlikeDriver::new(recovery),
                ParsingState::lang_initial_with_packages([math_envs]),
            )
        };

        let math = with_math_envs(Recovery::Strict)
            .parse("$\\begin{aligned}x\\end{aligned}$")
            .unwrap();
        check_tree_invariants(&math.tree);
        assert!(math.diagnostics.is_empty());

        let text = with_math_envs(Recovery::Tolerant)
            .parse("\\begin{aligned}x\\end{aligned}")
            .unwrap();
        assert_eq!(messages(&text), ["unknown environment ‘aligned’"]);
    }

    // --- the `"base"` seed: begin/end out of the box ------------------------------------

    #[test]
    fn begin_and_end_dispatch_out_of_the_box() {
        // No extra packages: `\begin`/`\end` are `"base"` entries; the environment
        // name is unknown, but the composition still parses the body to its
        // terminator under the tolerant fallback.
        let language = Language::new(
            LatexlikeDriver::new(Recovery::Tolerant),
            ParsingState::lang_initial(),
        );
        let result = language.parse("\\begin{itemize}x\\end{itemize}").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(messages(&result), ["unknown environment ‘itemize’"]);
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.environment_name(), Some("itemize"));
        assert_eq!(body_shapes(env), ["chars(x)"]);
    }

    #[test]
    fn unloading_base_removes_the_dispatch_pair() {
        let seed = ParsingState::<Latexlike>::lang_initial()
            .derived(
                &ParsingStateDelta::new().scope_op(ScopeOp::Unload { name: "base".into() }),
            )
            .unwrap();
        let language = Language::new(LatexlikeDriver::new(Recovery::Tolerant), seed);
        let result = language.parse("\\begin{itemize}").unwrap();
        let all = messages(&result);
        assert_eq!(all.len(), 1);
        assert!(all[0].contains("cannot resolve command ‘\\begin’"), "{}", all[0]);
        // `\begin` recovered as chars, `{itemize}` an ordinary group.
        assert_eq!(root_shapes(&result), ["chars(\\begin)", "group(Content { })"]);
    }

    // --- recovery matrix ---------------------------------------------------------------

    #[test]
    fn terminator_name_mismatch_unwinds() {
        let content = "\\begin{A}x\\begin{B}y\\end{A}";
        let result = parse_tolerant(content);
        let all = messages(&result);
        assert_eq!(all.len(), 1);
        assert!(
            all[0].contains("missing terminator of environment ‘B’")
                && all[0].contains("‘A’"),
            "{}",
            all[0]
        );
        // B closed without consuming `\end{A}`; A found and consumed its own
        // terminator.
        let outer = result.tree.root().child(0).unwrap();
        assert_eq!(outer.environment_name(), Some("A"));
        assert_eq!(outer.span().range(), 0..content.len());
        assert_eq!(body_shapes(outer), ["chars(x)", "Environment(B)"]);

        let err = strict().parse(content).unwrap_err();
        assert!(err.to_string().contains("missing terminator of environment ‘B’"), "{err}");
    }

    #[test]
    fn missing_terminator_at_end_of_input() {
        let result = parse_tolerant("\\begin{itemize}x");
        let all = messages(&result);
        assert_eq!(all.len(), 1);
        assert!(
            all[0].contains("missing terminator of environment ‘itemize’ before end of input"),
            "{}",
            all[0]
        );
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.span().range(), 0..16);
        assert_eq!(body_shapes(env), ["chars(x)"]);

        assert!(strict().parse("\\begin{itemize}x").is_err());
    }

    #[test]
    fn malformed_terminator_consumes_the_command_alone() {
        let result = parse_tolerant("\\begin{itemize}x\\end y");
        let all = messages(&result);
        assert_eq!(all.len(), 1);
        assert!(
            all[0].contains("malformed terminator of environment ‘itemize’"),
            "{}",
            all[0]
        );
        // The command (with its post-space) is consumed; `y` is sibling content
        // after the environment.
        assert_eq!(
            root_shapes(&result),
            ["Environment(itemize)", "chars(y)"]
        );
    }

    #[test]
    fn stray_group_close_in_the_body_unwinds_to_the_root() {
        // The root's tolerant stray-close recovery stages the consumed delimiter as
        // a chars node (7.9, superseding 7.4's byte-dropping quirk), so the partition
        // invariant holds across the unwind.
        let result = tolerant().parse("\\begin{itemize}a}b").unwrap();
        check_tree_invariants(&result.tree);
        let all = messages(&result);
        assert_eq!(all.len(), 2, "{all:?}");
        assert!(all[0].contains("missing terminator of environment ‘itemize’"), "{}", all[0]);
        assert!(all[1].contains("‘}’"), "{}", all[1]);
        assert_eq!(
            root_shapes(&result),
            ["Environment(itemize)", "chars(})", "chars(b)"]
        );
    }

    #[test]
    fn unknown_environments_still_parse_their_body() {
        let content = "\\begin{foo}x\\end{foo} y";
        let result = parse_tolerant(content);
        assert_eq!(messages(&result), ["unknown environment ‘foo’"]);
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.environment_name(), Some("foo"));
        assert_eq!(body_shapes(env), ["chars(x)"]);

        let err = strict().parse(content).unwrap_err();
        assert!(err.to_string().contains("unknown environment ‘foo’"), "{err}");
    }

    #[test]
    fn malformed_begin_recovers_as_chars() {
        let result = parse_tolerant("\\begin x");
        let all = messages(&result);
        assert_eq!(all.len(), 1);
        assert!(all[0].contains("malformed ‘\\begin’"), "{}", all[0]);
        // The chars fallback covers the consumed extent — the trigger *with* its
        // syntactic post-space, keeping the sibling partition exact.
        assert_eq!(root_shapes(&result), ["chars(\\begin )", "chars(x)"]);

        assert!(strict().parse("\\begin x").is_err());
    }

    #[test]
    fn orphan_end_at_the_root() {
        let result = parse_tolerant("a\\end{itemize}b");
        assert_eq!(
            messages(&result),
            ["orphan ‘\\end{itemize}’: no matching ‘\\begin{itemize}’"]
        );
        assert_eq!(
            root_shapes(&result),
            ["chars(a)", "chars(\\end{itemize})", "chars(b)"]
        );

        let err = strict().parse("a\\end{itemize}b").unwrap_err();
        assert!(err.to_string().contains("orphan ‘\\end{itemize}’"), "{err}");
    }

    #[test]
    fn orphan_end_without_a_name_group() {
        let result = parse_tolerant("\\end x");
        assert_eq!(messages(&result), ["orphan ‘\\end’: no matching ‘\\begin’"]);
        assert_eq!(root_shapes(&result), ["chars(\\end )", "chars(x)"]);
    }

    #[test]
    fn orphan_end_inside_a_group() {
        let result = parse_tolerant("{\\end{itemize}}");
        assert_eq!(
            messages(&result),
            ["orphan ‘\\end{itemize}’: no matching ‘\\begin{itemize}’"]
        );
        let group = result.tree.root().child(0).unwrap();
        assert!(group.is_group());
        assert_eq!(group.child(0).unwrap().summary(), "chars(\\end{itemize})");
    }

    #[test]
    fn bare_begin_in_expression_position_is_diagnosed() {
        // The documented divergence from pylatexenc (slots session): `\begin` as a
        // bare expression argument is diagnosed, not dispatched as the argument.
        let result = parse_tolerant("\\frac\\begin{itemize} x");
        let all = messages(&result);
        assert!(
            all.iter().any(|m| m.contains("requires content")),
            "{all:?}"
        );
    }

    // --- traceback vocabulary ----------------------------------------------------------

    #[test]
    fn argument_frames_quote_the_environment() {
        // An unresolvable command inside `tabular`'s mandatory argument: the
        // diagnostic's traceback quotes the environment's name, not `\begin`.
        let result = parse_tolerant("\\begin{tabular}{a\\foo}x\\end{tabular}");
        let diagnostic = result.diagnostics.iter().next().unwrap();
        let titles: Vec<&str> =
            diagnostic.frames().iter().map(|frame| frame.title()).collect();
        assert!(
            titles.contains(&"argument #1 of environment ‘tabular’"),
            "{titles:?}"
        );
        assert!(titles.contains(&"macro ‘\\begin’"), "{titles:?}");
    }

    #[test]
    fn body_frames_quote_the_environment() {
        // A condition inside the body: the body's frame quotes the environment.
        let result = parse_tolerant("\\begin{itemize}\\foo\\end{itemize}");
        let diagnostic = result.diagnostics.iter().next().unwrap();
        let titles: Vec<&str> =
            diagnostic.frames().iter().map(|frame| frame.title()).collect();
        assert!(titles.contains(&"environment ‘itemize’"), "{titles:?}");
    }

    // --- the spec surface --------------------------------------------------------------

    fn probe_invocation() -> EnvironmentInvocation<'static> {
        EnvironmentInvocation {
            trigger_span: Span::empty(0),
            name: "probe",
            name_span: Span::empty(0),
            escape_char: '\\',
            name_group_open: "{",
            name_group_close: "}",
        }
    }

    #[test]
    fn with_body_delta_overrides_any_behavior() {
        // On the declarative standard behavior…
        let spec = EnvironmentSpec::<Latexlike>::new(vec![]);
        assert!(spec.behavior().body_state_delta(probe_invocation()).is_none());
        let spec = spec.with_body_delta(ParsingStateDelta::new().mode(Mode::Math));
        let delta = spec.behavior().body_state_delta(probe_invocation()).unwrap();
        assert_eq!(delta.mode, Some(Mode::Math));

        // …and wrapping a custom behavior.
        #[derive(Debug)]
        struct Custom;
        impl EnvironmentBehavior for Custom {
            fn body_state_delta(
                &self,
                _invocation: EnvironmentInvocation<'_>,
            ) -> Option<ParsingStateDelta<Latexlike>> {
                Some(ParsingStateDelta::new().mode(Mode::Text))
            }
        }
        let custom = EnvironmentSpec::from_behavior(Arc::new(Custom))
            .with_body_delta(ParsingStateDelta::new().mode(Mode::Math));
        let delta = custom.behavior().body_state_delta(probe_invocation()).unwrap();
        assert_eq!(delta.mode, Some(Mode::Math));
    }

    #[test]
    fn environment_spec_exposes_arguments_through_the_callable_trait() {
        let spec = EnvironmentSpec::new(vec![brace_arg()]);
        let dyn_spec: &dyn CallableSpec<Latexlike> = &spec;
        assert_eq!(dyn_spec.arguments().len(), 1);
        assert_eq!(
            dyn_spec.stack_frame_title(FrameRole::Invocation, "align"),
            "environment ‘align’"
        );
        assert_eq!(
            dyn_spec.stack_frame_title(FrameRole::Argument { index: 0 }, "align"),
            "argument #1 of environment ‘align’"
        );
    }

    #[test]
    fn a_generic_callable_spec_under_environment_gets_default_body_handling() {
        // A non-`EnvironmentSpec` registration under the Environment type: declared
        // arguments parse, the body takes the default handling.
        let mut package = Package::new("generic");
        package.insert(
            CallableType::Environment,
            "gen",
            Arc::new(crate::spec::StdCallableSpec { arguments: vec![brace_arg()] }),
        );
        let language = Language::new(
            LatexlikeDriver::new(Recovery::Strict),
            ParsingState::lang_initial_with_packages([package]),
        );
        let result = language.parse("\\begin{gen}{a}x\\end{gen}").unwrap();
        check_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty());
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.environment_name(), Some("gen"));
        assert_eq!(env.arguments().unwrap().len(), 1);
        assert_eq!(body_shapes(env), ["chars(x)"]);
    }

    // --- verbatim bodies (7.7): `VerbatimBehavior` through the composition ------------

    #[test]
    fn verbatim_body_is_raw_text_with_the_newline_gobbled() {
        // pylatexenc test_latexnodes_parsers_verbatim test_simple, through the full
        // composition: escapes, `\begin`, comments, specials — all raw.
        let content = "\\begin{verbatim}\nHello world.\\\n\\macro, \\begin! % no comment; ~.\\( x\n\\end{verbatim}\n";
        let result = parse_ok(content);
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.environment_name(), Some("verbatim"));

        let evpos = content.find("\\end{verbatim}").unwrap();
        assert_eq!(env.span().range(), 0..evpos + "\\end{verbatim}".len());

        // Body content: everything between the gobbled newline and the terminator.
        let body: Vec<_> = env.body().unwrap().iter().collect();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].chars(), Some(&content[17..evpos]));
        // The raw chars node records the features-off verbatim state.
        assert!(!body[0].parsing_state().rules().enable_commands);
        assert!(!body[0].parsing_state().rules().enable_comments);

        // The gobbled newline is kept as the body list's first child — every byte
        // stays in the tree — but designated out of the content.
        let list = env.slot_content_parent(0).unwrap();
        assert_eq!(list.child_count(), 2);
        assert_eq!(list.child(0).unwrap().chars(), Some("\n"));

        // The newline after `\end{verbatim}` is ordinary sibling content.
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("\n"));
    }

    #[test]
    fn verbatim_at_stream_end_and_without_a_leading_newline() {
        // pylatexenc test_simple_nofinaleol; no leading newline means no gobble.
        let result = parse_ok("\\begin{verbatim}abc %x\\end{verbatim}");
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.span().range(), 0..36);
        assert_eq!(body_shapes(env), ["chars(abc %x)"]);
        let list = env.slot_content_parent(0).unwrap();
        assert_eq!(list.child_count(), 1);
    }

    #[test]
    fn empty_and_newline_only_verbatim_bodies() {
        let result = parse_ok("\\begin{verbatim}\\end{verbatim}");
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(body_shapes(env), Vec::<String>::new());
        assert_eq!(env.slot_content_parent(0).unwrap().child_count(), 0);

        // A body that is just the gobbled newline: the node exists, the content is
        // empty.
        let result = parse_ok("\\begin{verbatim}\n\\end{verbatim}");
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(body_shapes(env), Vec::<String>::new());
        assert_eq!(env.slot_content_parent(0).unwrap().child_count(), 1);
    }

    #[test]
    fn verbatim_terminator_matching_is_literal() {
        // `\end {verbatim}` (with a space) is not the terminator — the raw scan
        // matches the literal string, pylatexenc's string-search parity — so the body
        // runs to end of input and recovers.
        let result = parse_tolerant("\\begin{verbatim}a\\end {verbatim}");
        assert_eq!(
            messages(&result),
            ["missing terminator of environment ‘verbatim’ before end of input"]
        );
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(body_shapes(env), ["chars(a\\end {verbatim})"]);
    }

    #[test]
    fn verbatim_does_not_nest() {
        // An inner `\begin{verbatim}` is raw text; the first terminator ends the body.
        let result = parse_ok("\\begin{verbatim}a\\begin{verbatim}b\\end{verbatim} c");
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(body_shapes(env), ["chars(a\\begin{verbatim}b)"]);
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some(" c"));
    }

    #[test]
    fn starred_verbatim_terminates_on_its_own_name() {
        // `verbatim*` is an ordinary separate entry; the terminator back-references
        // the invocation's name, `*` included.
        let result = parse_ok("\\begin{verbatim*}a b\\end{verbatim*}");
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.environment_name(), Some("verbatim*"));
        assert_eq!(body_shapes(env), ["chars(a b)"]);
    }

    #[test]
    fn lstlisting_style_arguments_parse_before_the_raw_body() {
        // The option group parses tokenized, the raw body follows; the gobbled
        // newline lives inside the body list, so the callable's children block stays
        // span-contiguous (option group, then list — invariant 3).
        let content = "\\begin{lstlisting}[language=Python]\nif a<b: pass\n\\end{lstlisting}";
        let result = parse_ok(content);
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.child_count(), 2);
        assert!(env.arguments().unwrap().get(0).unwrap().is_provided());
        let option: Vec<_> = env.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(option.len(), 1);
        assert_eq!(option[0].chars(), Some("language=Python"));
        assert_eq!(body_shapes(env), ["chars(if a<b: pass\n)"]);
    }

    #[test]
    fn unterminated_verbatim_environment_recovers() {
        let err = strict().parse("\\begin{verbatim}\nabc").unwrap_err();
        assert!(
            err.to_string().contains("missing terminator of environment ‘verbatim’"),
            "{err}"
        );

        let result = parse_tolerant("\\begin{verbatim}\nabc");
        assert_eq!(
            messages(&result),
            ["missing terminator of environment ‘verbatim’ before end of input"]
        );
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.span().range(), 0..20);
        assert_eq!(body_shapes(env), ["chars(abc)"]);
    }
}
