//! Environments: the `\begin{name} … \end{name}` composition and its spec surface.
//!
//! The *notion* of "environment" is preset property; core contributes parameterized
//! building blocks only. This module promotes a composition once rehearsed test-side:
//!
//! - [`BeginSpec`] — the `\begin` dispatcher, registered as an ordinary
//!   [`Macro`](CallableType::Macro) entry of the [`builtin_package`](super::builtin_package)
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
//! Neither command name of the pair is fixed here: the opening one is whatever the
//! [`BeginSpec`] entry is registered under, and the terminator one is
//! [`BeginSpec::new`]'s argument — `\begin`/`\end` is what
//! [`builtin_package`](super::builtin_package) happens to register. The diagnostics
//! this module raises quote the spellings the source used.
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
//!     ParsingState::lang_initial_with_packages([package]).expect("seed state"),
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
use core::any::Any;
use core::fmt;

use crate::constructs::{
    parse_declared_arguments, read_rigid_name_group, ConstructParser,
    ConstructParserResult, EnvironmentBeginSyntaxData, EnvironmentBody,
    EnvironmentBodyParser, Invocation, ParseContext, VerbatimBodyParser,
    VerbatimBodyTerminator,
};
use crate::error::{DiagnosticInfo, ParseError};
use crate::node::{
    BodySlotExt, BuildId, CallableData, ChildRegion, NodeKind, ParsedArguments,
    ParsedSlot, ParsedSlots, SlotRole,
};
use core::marker::PhantomData;

use crate::scopes::{CallableQuery, CallableSyntax, SpecProvenance};
use crate::source::{SourceSpan, TextContent};
use crate::spec::{ArgumentSpec, CallableSpec, FrameRole};
use crate::state::ParsingStateDelta;
use crate::token::{GroupRule, TokenEdge};

use super::invocation_syntax::EnvironmentSyntax;
use super::lang::{
    LatexlikeCallableType, LatexlikeGroupType, LatexlikeInvocationSyntax, LatexlikeLang,
};
use super::spec::frame_title;
use super::Latexlike;

// --- conditions --------------------------------------------------------------------

/// Condition: the environment-opening command was not followed immediately by its
/// rigid name group (`\begin [x]`, `\begin{ itemize }`). Tolerant recovery stages the
/// trigger alone — its syntactic post-space included, keeping the sibling partition
/// exact — as a `Chars` node (the accepted markup-in-chars recovery artifact) and
/// consumes nothing past it.
///
/// The message quotes the command as it was written: the opening command's name is
/// [`BeginSpec`]'s registration name, `\begin` only by convention.
///
/// Like every condition, it is constructible outside the crate (e.g. for
/// manufacturing diagnostics in an embedding's own tests):
///
/// ```
/// use techy::latexlike::MalformedBegin;
///
/// let condition = MalformedBegin::new("\\begin");
/// assert_eq!(condition, MalformedBegin::new("\\begin"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "latexlike.environments.malformed-begin",
    message = "malformed ‘{command}’: expected the environment's name group \
               immediately after the command"
)]
pub struct MalformedBegin {
    /// The opening command as written, escape character included and the trigger's
    /// syntactic post-space excluded (`\begin`).
    pub command: String,
}

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

/// Condition: an environment terminator with no environment open at its level.
/// Inside a body the terminator is consumed by the body parser before command
/// resolution, so a dispatched terminator is always an orphan ([`EndSpec`]). Tolerant
/// recovery stages the consumed extent — `\end{name}` whole, or the command alone when
/// the name group is malformed — as a `Chars` node.
///
/// The message quotes the terminator as it was written; nothing here spells the
/// *opening* command, which this site has no way to know (the pairing is
/// [`BeginSpec`]'s, and only in the opening direction).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "latexlike.environments.orphan-end")]
pub struct OrphanEnd {
    /// The environment named by the terminator, when its name group parsed.
    pub name: Option<String>,
    /// The terminator as written: the whole consumed extent (`\end{align}`), or the
    /// command alone — post-space excluded — when its name group was malformed
    /// (`\end`).
    pub terminator: String,
}

// Hand-written wording: the message names the environment only when the name group
// parsed (a match, which the message format string cannot express).
impl fmt::Display for OrphanEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let terminator = &self.terminator;
        match &self.name {
            Some(name) => {
                write!(f, "orphan ‘{terminator}’: no environment ‘{name}’ is open here")
            }
            None => write!(f, "orphan ‘{terminator}’: no environment is open here"),
        }
    }
}

// --- the environment spec surface --------------------------------------------------

/// The invocation facts of one environment being parsed — what
/// [`EnvironmentBehavior`]'s hooks receive from the `\begin` composition. Grows by
/// field as behavior hooks demand (`#[non_exhaustive]`); built only by the
/// composition.
#[non_exhaustive]
pub struct EnvironmentInvocation<'p, LLL: LatexlikeLang = Latexlike> {
    /// The `\begin` trigger token's span — the anchor of body-level diagnostics
    /// (missing terminator).
    pub trigger_span: SourceSpan<LLL::SourceOrigin>,
    /// The environment's name as written inside the name group (`itemize`,
    /// `figure*`).
    pub name: &'p str,
    /// The name's span (the name group's interior).
    pub name_span: SourceSpan<LLL::SourceOrigin>,
    /// The begin trigger's escape character as written — the canonical escape a
    /// takeover body composes its terminator spelling from
    /// ([`VerbatimBehavior`]'s literal `\end{name}`).
    pub escape_char: char,
    /// The begin name group's open delimiter as written (off the matched rule).
    pub name_group_open: &'p str,
    /// The begin name group's close delimiter as written.
    pub name_group_close: &'p str,
    /// The terminator command's name (`end`), from the dispatching
    /// [`BeginSpec`](BeginSpec::end_command_name): the body's stop condition, and the
    /// command word a takeover body composes its terminator spelling from.
    pub end_command_name: &'p str,
}

// Manual impls: derives would demand `LLL: Clone`/`LLL: Debug` although only spans
// and borrowed strings are held.

impl<LLL: LatexlikeLang> Clone for EnvironmentInvocation<'_, LLL> {
    fn clone(&self) -> Self {
        EnvironmentInvocation {
            trigger_span: self.trigger_span.clone(),
            name: self.name,
            name_span: self.name_span.clone(),
            escape_char: self.escape_char,
            name_group_open: self.name_group_open,
            name_group_close: self.name_group_close,
            end_command_name: self.end_command_name,
        }
    }
}

impl<LLL: LatexlikeLang> fmt::Debug for EnvironmentInvocation<'_, LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnvironmentInvocation")
            .field("trigger_span", &self.trigger_span)
            .field("name", &self.name)
            .field("name_span", &self.name_span)
            .field("escape_char", &self.escape_char)
            .field("name_group_open", &self.name_group_open)
            .field("name_group_close", &self.name_group_close)
            .field("end_command_name", &self.end_command_name)
            .finish()
    }
}

/// The behavior of one environment, behind [`EnvironmentSpec`] — the wrapper's inner
/// trait: third-party implementations override the
/// defaulted methods; the composition reaches them through the concrete wrapper's
/// downcast. The pylatexenc `EnvironmentSpec` analog (`make_body_parser`,
/// `make_body_parsing_state_delta`), with the declarative standard implementation
/// behind [`EnvironmentSpec::new`].
///
/// **Downcasting is part of the contract** (`Any` supertrait;
/// [`CallableSpec`]'s downcasting note applies): a consumer recovers a behavior's
/// concrete type from the stored `Arc<dyn EnvironmentBehavior<LLL>>` or
/// `&dyn EnvironmentBehavior<LLL>`.
pub trait EnvironmentBehavior<LLL: LatexlikeLang = Latexlike>:
    fmt::Debug + Send + Sync + Any
{
    /// The declarative argument structure of the environment, in invocation order —
    /// parsed right after `\begin{name}`, before the body. Default:
    /// no arguments.
    fn arguments(&self) -> &[Arc<ArgumentSpec<LLL>>] {
        &[]
    }

    /// The body's parsing-state delta (mode changes, tokenization tweaks), stacked on
    /// the invocation's base state for the body's whole extent — terminator included —
    /// and reverted structurally after (pylatexenc's `make_body_parsing_state_delta`).
    /// Default: none (`Ok(None)`).
    ///
    /// # Errors
    ///
    /// `Err` **aborts the parse** under any recovery policy — the composition
    /// derives the body state from the answer, and there is no recovery channel
    /// at this seam; the composition attaches the live traceback when the error
    /// carries no frames of its own. Carry
    /// [`HookFailed`](crate::error::HookFailed) for an operational failure in the
    /// behavior's own code,
    /// [`ImplementationError`](crate::constructs::ImplementationError) for a
    /// violated library contract, or a document condition for a diagnosis made
    /// deliberately (a behavior reading malformed argument data). An infallible
    /// implementation wraps its delta in `Ok(...)` and that is the only change.
    fn body_state_delta(
        &self,
        invocation: EnvironmentInvocation<'_, LLL>,
    ) -> Result<Option<ParsingStateDelta<LLL>>, ParseError<LLL::SourceOrigin>> {
        let _ = invocation;
        Ok(None)
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
        invocation: EnvironmentInvocation<'p, LLL>,
    ) -> Box<dyn ConstructParser<LLL, Output = EnvironmentBody<LLL>> + 'p> {
        default_body_parser(invocation)
    }
}

/// The default body of [`EnvironmentBehavior::make_body_parser`], shared with the
/// composition's non-[`EnvironmentSpec`] fallback: the core [`EnvironmentBodyParser`]
/// over the preset's terminator shape, stopping on the invocation's own terminator
/// command.
fn default_body_parser<'p, LLL: LatexlikeLang>(
    invocation: EnvironmentInvocation<'p, LLL>,
) -> Box<dyn ConstructParser<LLL, Output = EnvironmentBody<LLL>> + 'p> {
    Box::new(
        EnvironmentBodyParser::new(
            invocation.trigger_span.clone(),
            invocation.name,
            invocation.end_command_name,
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
/// [`VerbatimBodyParser`] up to the `\end{name}` terminator, given to it as a
/// [`StopEnvironmentCommand`](VerbatimBodyTerminator::StopEnvironmentCommand)
/// terminator built from the invocation's own spellings (the escape character it was
/// written with, its name group's delimiters, and the terminator command name the
/// dispatching [`BeginSpec`] carries — the same spellings the `\begin` composition
/// itself is built on). The single newline right
/// after the begin syntax is staged but designated out of the body content
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
///     ParsingState::lang_initial_with_packages([package]).expect("seed state"),
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
        invocation: EnvironmentInvocation<'p, LLL>,
    ) -> Box<dyn ConstructParser<LLL, Output = EnvironmentBody<LLL>> + 'p> {
        // The terminator's pieces, taken from the invocation's own spellings (the
        // begin trigger's escape char and the begin name group's delimiters): the
        // parser composes `\end{name}` from them to read the body up to, and reports
        // the very same pieces back as the standard end facts. The name group rule is
        // minted here rather than carried off the matched begin token — the delimiter
        // bytes and the group class are all that is recorded from it.
        Box::new(
            VerbatimBodyParser::new(
                invocation.trigger_span.clone(),
                invocation.name,
                VerbatimBodyTerminator::StopEnvironmentCommand {
                    escape_char: invocation.escape_char,
                    invocation_name: invocation.name,
                    stop_command_name: invocation.end_command_name,
                    name_group_rule: Arc::new(GroupRule {
                        group_type: LLL::GroupTypeId::content_group(),
                        open: invocation.name_group_open.into(),
                        close: invocation.name_group_close.into(),
                    }),
                },
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

    /// Infallible: `Ok(...)` wrapping is this implementation's whole use of the
    /// `Result`.
    fn body_state_delta(
        &self,
        _invocation: EnvironmentInvocation<'_, LLL>,
    ) -> Result<Option<ParsingStateDelta<LLL>>, ParseError<LLL::SourceOrigin>> {
        Ok(Some(self.delta.clone()))
    }

    fn make_body_parser<'p>(
        &'p self,
        invocation: EnvironmentInvocation<'p, LLL>,
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
/// [`CallableType::Environment`](super::CallableType::Environment) entries — the concrete wrapper through which the `\begin` composition reaches the environment's
/// [`EnvironmentBehavior`] (`Any` downcasts hit concrete types only, so the open set
/// of behaviors is reached through this one concrete spec type).
///
/// The composition — not this spec — parses the invocation:
/// [`make_invocation_parser`](CallableSpec::make_invocation_parser) is never
/// consulted for environment entries (a deliberate, permanent boundary; registering an
/// `EnvironmentSpec` under another callable type gets the macro-shaped default
/// parse, which reads the arguments and no body). A generic non-`EnvironmentSpec`
/// [`CallableSpec`] under [`CallableType::Environment`](super::CallableType::Environment) is legitimate too: its
/// declared arguments parse and the body takes the default handling.
///
/// **Serialization.** The behavior (its arguments' parsers, its body handling) has no
/// serialized form, so an environment spec is serialized by *identity* — a reference
/// to the provider that defined it plus its key — which needs the [`SpecProvenance`]
/// stamp a shared package hands out ([`with_provenance`](EnvironmentSpec::with_provenance);
/// [`Package::define_environment`](crate::scopes::Package::define_environment) stamps
/// automatically in a shared package). An unstamped environment spec cannot be
/// serialized (the error names the type).
pub struct EnvironmentSpec<LLL: LatexlikeLang = Latexlike> {
    behavior: Arc<dyn EnvironmentBehavior<LLL>>,
    provenance: Option<SpecProvenance<LLL>>,
}

impl<LLL: LatexlikeLang> EnvironmentSpec<LLL> {
    /// A declarative environment with the given argument structure and the default
    /// body handling.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<LLL>>>) -> EnvironmentSpec<LLL> {
        EnvironmentSpec::from_behavior(Arc::new(StdEnvironmentBehavior { arguments }))
    }

    /// An environment driven by a custom [`EnvironmentBehavior`] — the wrapper's
    /// registration entry for behavior-shaped customization (verbatim-like bodies).
    pub fn from_behavior(behavior: Arc<dyn EnvironmentBehavior<LLL>>) -> EnvironmentSpec<LLL> {
        EnvironmentSpec { behavior, provenance: None }
    }

    /// Set the body's parsing-state delta (`equation` entering
    /// [`Mode::Math`](super::Mode), a listing disabling comment tokenization),
    /// overriding whatever the current behavior computes.
    pub fn with_body_delta(self, delta: ParsingStateDelta<LLL>) -> EnvironmentSpec<LLL> {
        EnvironmentSpec {
            behavior: Arc::new(BodyDeltaOverride { inner: self.behavior, delta }),
            provenance: self.provenance,
        }
    }

    /// Record where this spec is defined — the [`SpecProvenance`] stamp a shared
    /// package hands out ([`Package::provenance_for`](crate::scopes::Package::provenance_for))
    /// — so that the spec can be serialized by identity. Replaces a previous stamp.
    pub fn with_provenance(mut self, provenance: SpecProvenance<LLL>) -> EnvironmentSpec<LLL> {
        self.provenance = Some(provenance);
        self
    }

    /// Where this spec is defined, if it was stamped.
    pub fn provenance(&self) -> Option<&SpecProvenance<LLL>> {
        self.provenance.as_ref()
    }

    /// The behavior driving this environment's parse.
    pub fn behavior(&self) -> &dyn EnvironmentBehavior<LLL> {
        &*self.behavior
    }
}

// The `SerializableObject` impl (identity through the provenance stamp) lives in
// `super::serialize`, with the preset's other serialization impls.

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
        EnvironmentSpec { behavior: Arc::clone(&self.behavior), provenance: self.provenance.clone() }
    }
}

impl<LLL: LatexlikeLang> fmt::Debug for EnvironmentSpec<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnvironmentSpec")
            .field("behavior", &self.behavior)
            .field("provenance", &self.provenance)
            .finish()
    }
}

// --- the `\begin` dispatcher and its composition ------------------------------------

/// The `\begin` dispatcher: every environment enters through this shared spec, an
/// ordinary [`Macro`](super::CallableType::Macro) entry of the
/// [`builtin_package`](super::builtin_package) (shadowable and unloadable like any
/// definition). Its parser is the preset's environment composition, which reads the
/// rigid `\begin{name}` syntax ([`read_rigid_name_group`](crate::constructs::read_rigid_name_group)),
/// resolves the environment's own definition, parses its declared arguments and its
/// body ([`EnvironmentBodyParser`](crate::constructs::EnvironmentBodyParser)), and
/// stages the environment node.
///
/// The spec carries the **terminator command's name** — the `end` of `\end{name}` —
/// rather than assuming it: the opening command's own name is whatever the definition
/// is registered under, and the closing one is this field, so a language spelling the
/// pair `\open`/`\shut` registers `BeginSpec::new("shut")` under `"open"` and needs no
/// code of its own. The name reaches the body parsers through
/// [`EnvironmentInvocation::end_command_name`].
///
/// **Serialization.** A `BeginSpec` has a self-contained serialized form (the
/// terminator command's name), so it is always serializable; when it carries a
/// [`SpecProvenance`] stamp ([`with_provenance`](BeginSpec::with_provenance) — the
/// [`builtin_package`](super::builtin_package) stamps its `\begin`) it is serialized
/// by identity instead, so that reading yields the very instance the reading side's
/// package holds.
pub struct BeginSpec<LLL: LatexlikeLang = Latexlike> {
    end_command_name: String,
    provenance: Option<SpecProvenance<LLL>>,
    lang: PhantomData<fn() -> LLL>,
}

impl<LLL: LatexlikeLang> BeginSpec<LLL> {
    /// The environment dispatcher spec whose bodies end at the command
    /// `end_command_name` (`"end"` for `\end{name}` — written without the escape
    /// character, which is the invocation's).
    ///
    /// The name must be the one the terminator command is *recognized* under: the
    /// body parsers match it against the command token's name, so a name no command
    /// token can carry leaves every body running to the end of its input (diagnosed
    /// as a missing terminator). Registering an [`EndSpec`] under the same name is
    /// what turns a stray terminator into an [`OrphanEnd`] diagnostic rather than an
    /// unknown command; nothing enforces the pairing.
    pub fn new(end_command_name: impl Into<String>) -> BeginSpec<LLL> {
        BeginSpec { end_command_name: end_command_name.into(), provenance: None, lang: PhantomData }
    }

    /// The terminator command's name, as given to [`new`](BeginSpec::new).
    pub fn end_command_name(&self) -> &str {
        &self.end_command_name
    }

    /// Record where this spec is defined — the [`SpecProvenance`] stamp a shared
    /// package hands out — so that the spec is serialized by identity rather than in
    /// its self-contained form. Replaces a previous stamp.
    pub fn with_provenance(mut self, provenance: SpecProvenance<LLL>) -> BeginSpec<LLL> {
        self.provenance = Some(provenance);
        self
    }

    /// Where this spec is defined, if it was stamped.
    pub fn provenance(&self) -> Option<&SpecProvenance<LLL>> {
        self.provenance.as_ref()
    }
}

// The `SerializableObject`/`DeserializableObject` impls live in `super::serialize`.

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

    /// Infallible: `Ok(...)` wrapping is this implementation's whole use of the
    /// `Result`.
    fn make_invocation_parser<'a>(
        &'a self,
        invocation: Invocation<'a, LLL>,
    ) -> Result<
        Box<dyn ConstructParser<LLL, Output = BuildId> + 'a>,
        crate::error::ParseError<LLL::SourceOrigin>,
    >
    {
        Ok(Box::new(EnvironmentInvocationParser {
            invocation,
            end_command_name: &self.end_command_name,
        }))
    }

    /// `\begin` itself is macro-shaped; the environment's own name titles the frames
    /// pushed once the composition has read it.
    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        frame_title("macro", role, name)
    }
}

// No `Default`/`Copy`: the terminator command name has no default worth assuming
// (that assumption is what this field exists to remove), and an owned name is not a
// `Copy` value. `Clone` stays — a spec is registration data.
impl<LLL: LatexlikeLang> Clone for BeginSpec<LLL> {
    fn clone(&self) -> Self {
        BeginSpec {
            end_command_name: self.end_command_name.clone(),
            provenance: self.provenance.clone(),
            lang: PhantomData,
        }
    }
}

impl<LLL: LatexlikeLang> fmt::Debug for BeginSpec<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BeginSpec")
            .field("end_command_name", &self.end_command_name)
            .field("provenance", &self.provenance)
            .finish()
    }
}

/// The orphan-`\end` spec: a resolved `\end` never belongs to an environment (the
/// body parser consumes well-formed terminators before command resolution), so its
/// parser diagnoses [`OrphanEnd`] and recovers. An ordinary
/// [`Macro`](super::CallableType::Macro) entry of the [`builtin_package`](super::builtin_package),
/// alongside [`BeginSpec`].
///
/// The spec carries no name of its own — it diagnoses whatever command it was
/// registered under, and that name should be the one the paired
/// [`BeginSpec`](BeginSpec::new) was given as its terminator.
///
/// **Serialization.** Stateless, so it is serialized in its (empty) self-contained
/// form and rebuilt as a fresh `EndSpec` — no provenance stamp (the type stays
/// `Copy`).
pub struct EndSpec<LLL: LatexlikeLang = Latexlike> {
    lang: PhantomData<fn() -> LLL>,
}

impl<LLL: LatexlikeLang> EndSpec<LLL> {
    /// The orphan-terminator diagnoser spec.
    pub fn new() -> EndSpec<LLL> {
        EndSpec { lang: PhantomData }
    }
}

// The `SerializableObject`/`DeserializableObject` impls live in `super::serialize`.

impl<LLL: LatexlikeLang> CallableSpec<LLL> for EndSpec<LLL> {
    /// Like `\begin`: declares nothing, reads material (its name group) — bare
    /// expression use is diagnosed.
    fn requires_content(&self) -> bool {
        true
    }

    /// Infallible: `Ok(...)` wrapping is this implementation's whole use of the
    /// `Result`.
    fn make_invocation_parser<'a>(
        &'a self,
        invocation: Invocation<'a, LLL>,
    ) -> Result<
        Box<dyn ConstructParser<LLL, Output = BuildId> + 'a>,
        crate::error::ParseError<LLL::SourceOrigin>,
    >
    {
        Ok(Box::new(OrphanEndParser { invocation }))
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
        *self
    }
}

impl<LLL: LatexlikeLang> Copy for EndSpec<LLL> {}

impl<LLL: LatexlikeLang> fmt::Debug for EndSpec<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EndSpec").finish()
    }
}

/// The environment composition (a tier-2 temporary, [`BeginSpec`]'s parser):
/// scaffolding, resolution, arguments, body, node assembly — assembled from the
/// public core building blocks (module docs). The composition owns **all
/// scanning**: it validates the trigger, reads the rigid name group
/// ([`read_rigid_name_group`]), parses arguments and body, and hands the
/// collected facts to the environment record's constructor
/// ([`EnvironmentSyntax::from_parsed`]) once, at staging time.
///
/// # Contract: std environments are command-initiated
///
/// This parser is dispatched for the `\begin` **command** ([`BeginSpec`] is a
/// macro-shaped entry), and its trigger must be a
/// [`Command`](crate::token::TokenKind::Command) token — a different trigger
/// shape (a specials-dispatched begin, say) is a documented-contract violation
/// and aborts as an implementation error. A custom trigger shape needs its own
/// composition *and* its own `Env` record type: this composition's begin facts
/// ([`EnvironmentBeginSyntaxData`]) are command-spelling facts by construction.
struct EnvironmentInvocationParser<'a, LLL: LatexlikeLang> {
    invocation: Invocation<'a, LLL>,
    /// The dispatching [`BeginSpec`]'s terminator command name, passed on to the
    /// body parsers through [`EnvironmentInvocation::end_command_name`].
    end_command_name: &'a str,
}

impl<LLL: LatexlikeLang> ConstructParser<LLL> for EnvironmentInvocationParser<'_, LLL>
where
    crate::node::SlotExt<LLL>: BodySlotExt,
{
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, LLL>,
    ) -> ConstructParserResult<LLL, (BuildId, Option<Box<ParsingStateDelta<LLL>>>)>
    {
        // The language's environment-side record ([`LatexlikeInvocationSyntax::Env`])
        // records the begin/end syntax; the composition owns all scanning plus
        // resolution, arguments, and node assembly.
        type Env<LLL> =
            <<LLL as crate::state::Lang>::InvocationSyntax as LatexlikeInvocationSyntax<
                LLL,
            >>::Env;
        let trigger = self.invocation.token;

        // The trigger contract first (see the type docs): std environments are
        // command-initiated, so a non-command trigger is a documented-contract
        // violation by whatever dispatched this composition — an implementation
        // error, not a source condition.
        let crate::token::TokenKind::Command { escape_char, .. } =
            cx.tokens.token_kind(trigger)
        else {
            return Err(cx.implementation_error(
                "the std environment composition requires a Command trigger \
                 (custom trigger shapes need their own composition and Env type)",
                cx.tokens.source_span_of(trigger),
            ));
        };
        // The trigger's own spelling, as the reader answers it: the command
        // itself (escape character included) and the syntactic post-space after it.
        let trigger_span = cx.tokens.source_span_of(trigger);
        let trigger_start = cx.tokens.position_at(trigger, TokenEdge::Start);
        let command =
            cx.tokens.source_span_between(trigger, TokenEdge::Start, TokenEdge::End);
        let post_space = cx.tokens.source_span_between(
            trigger,
            TokenEdge::End,
            TokenEdge::EndPastPostSpace,
        );

        // The begin-side scaffolding scan, composition-owned: the rigid name
        // group must be the immediately next token, of the language's content
        // class ([`read_rigid_name_group`]'s contract).
        let Some(name_group) =
            read_rigid_name_group(cx, LLL::GroupTypeId::content_group())?
        else {
            // The condition quotes the command as written — escape character
            // included, the trigger's own post-space excluded (`\begin` out of
            // `\begin [x]`) — since the opening command's name is this spec's
            // registration name, not a fixed spelling.
            cx.recover(
                MalformedBegin::new(String::from(command.content())),
                trigger_span.clone(),
            )?;
            // Chars fallback over the trigger alone (markup in a Chars node is the
            // accepted tolerant-recovery artifact); nothing past it is consumed.
            let id = cx
                .stage_node(
                    NodeKind::chars(trigger_span.span()),
                    trigger_span.clone(),
                    Arc::clone(&cx.state),
                    vec![],
                )
                .map_err(|error| cx.implementation_error(error, trigger_span))?;
            return Ok((id, None));
        };
        // The name as read, never the span's content: the span is only what the reader
        // described for the stretch when the language does not obey span tiling, and
        // the name drives the spec lookup, the node's `name` and the diagnostics.
        let name = name_group.name_text();

        // The begin side's spelling facts (escape char, command word, post-space,
        // matched name group) — recorded, no longer normalized away; handed to the
        // record's constructor at staging time.
        // The command *word* is the command minus its escape character, which is the
        // command's first character by construction.
        let begin_syntax = EnvironmentBeginSyntaxData {
            escape_char,
            command_word: SourceSpan::new(
                command.source(),
                (command.start() + escape_char.len_utf8())..command.end(),
            ),
            post_space,
            name_group: name_group.clone(),
        };

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
            .map_err(|error| cx.implementation_error(error, name_group.name_span().clone()))?;
        let spec: Arc<dyn CallableSpec<LLL>> = match resolved {
            Some(spec) => spec,
            None => {
                cx.recover(UnknownEnvironment::new(name), name_group.name_span().clone())?;
                // Tolerant fallback: an argument-less body-only environment, so the
                // body still parses to its terminator.
                Arc::new(EnvironmentSpec::<LLL>::new(vec![]))
            }
        };

        // Declared arguments: the shared core loop; the argument frames quote the
        // *environment's* name, not `\begin`.
        let (mut children, arguments) =
            parse_declared_arguments(cx, &spec, name_group.name_span())?;

        // The environment's behavior, through the funnel downcast. A
        // non-`EnvironmentSpec` registration has no behavior to offer and gets the
        // default body handling.
        let behavior: Option<&dyn EnvironmentBehavior<LLL>> = (&*spec
            as &dyn core::any::Any)
            .downcast_ref::<EnvironmentSpec<LLL>>()
            .map(|environment_spec| environment_spec.behavior());
        // The invocation facts handed to the behavior hooks, spelling pieces
        // included (a takeover body composes its terminator from them). The rule
        // `Arc` clone pins the delimiter strings for the borrow; the escape char
        // transcribes from the already-validated command trigger; the terminator
        // command name comes from the dispatching spec.
        let name_group_rule = Arc::clone(name_group.rule());
        let env_invocation = EnvironmentInvocation {
            trigger_span: trigger_span.clone(),
            name,
            name_span: name_group.name_span().clone(),
            escape_char,
            name_group_open: &name_group_rule.open,
            name_group_close: &name_group_rule.close,
            end_command_name: self.end_command_name,
        };

        // The body: parsed under the behavior's state delta stacked on the
        // invocation's base (session-mediated, structurally reverted), by the
        // behavior's body parser — content up to and including the `\end{name}`
        // terminator in the default shape. A hook Err aborts under any policy
        // (body_state_delta's contract), with the live traceback attached here.
        let body_delta = match behavior {
            Some(b) => b
                .body_state_delta(env_invocation.clone())
                .map_err(|error| cx.attach_hook_frames(error))?,
            None => None,
        };
        let body_state = match &body_delta {
            Some(delta) => cx.derive_state(delta)?,
            None => Arc::clone(&cx.state),
        };
        let mut body_parser = match behavior {
            Some(b) => b.make_body_parser(env_invocation),
            None => default_body_parser(env_invocation),
        };
        let (body, passthrough) =
            cx.parse_construct(&mut *body_parser, Some(body_state), None)?;
        drop(body_parser);
        // A behavior-supplied body parser is outer-layer input; its documented
        // contract (no pass-through delta) is enforced as an implementation
        // error, never a panic ([§dd-dr:panic-policy]).
        if passthrough.is_some() {
            return Err(cx.implementation_error(
                "the environment body parser must return no pass-through state delta",
                trigger_span,
            ));
        }

        // The payload, constructed once at staging time: the begin facts the
        // composition scanned plus the terminator facts the body parser (the
        // terminator consumer) reported back — the full command-plus-name-group
        // spelling for a tokenized terminator, and for a raw (verbatim) body too,
        // which consumed its terminator as one token but was given its pieces; a
        // body that closed without a terminator (mismatch, malformed, end of input)
        // leaves the end side empty.
        // The node's own extent, needed before the payload: the record converts each
        // source-qualified fact it was handed into node data against it.
        let node_span = cx.source_span_within(&trigger_start, &body.end)?;
        let env_syntax = Env::<LLL>::from_parsed(begin_syntax, body.terminator, &node_span);

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
        let id = cx
            .stage_node(
                NodeKind::callable(data),
                node_span.clone(),
                Arc::clone(&cx.state),
                children,
            )
            .map_err(|error| cx.staging_error(error, node_span))?;
        Ok((id, None))
    }
}

/// The orphan-`\end` recovery parser ([`EndSpec`]'s): read the name group when
/// present, diagnose [`OrphanEnd`], stage the consumed extent as a `Chars` node.
struct OrphanEndParser<'a, LLL: LatexlikeLang> {
    invocation: Invocation<'a, LLL>,
}

impl<LLL: LatexlikeLang> ConstructParser<LLL> for OrphanEndParser<'_, LLL> {
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, LLL>,
    ) -> ConstructParserResult<LLL, (BuildId, Option<Box<ParsingStateDelta<LLL>>>)>
    {
        let trigger = self.invocation.token;
        let trigger_start = cx.tokens.position_at(trigger, TokenEdge::Start);
        // The command word's end, for the quoted spelling of a terminator whose name
        // group never parsed: a command trigger's own post-space is consumed with it
        // and would read as a trailing blank inside the quotes. Any other trigger
        // shape (this spec is registrable under any syntax) quotes its whole extent.
        let command_end = match cx.tokens.token_kind(trigger) {
            crate::token::TokenKind::Command { .. } => TokenEdge::End,
            _ => TokenEdge::EndPastPostSpace,
        };
        let after_trigger = cx.tokens.position_at(trigger, TokenEdge::EndPastPostSpace);
        let name_group = read_rigid_name_group(cx, LLL::GroupTypeId::content_group())?;
        let (name, end) = match &name_group {
            // The name as read, never the span's content (see the begin side).
            Some(group) => (Some(String::from(group.name_text())), group.end().clone()),
            // Malformed name group: nothing past the trigger was consumed.
            None => (None, after_trigger),
        };
        let span = cx.source_span_within(&trigger_start, &end)?;
        // What was consumed, as text: the trigger up to `edge` — one reader answer
        // about one token, taken from its `Start` so the pre-space the content loop
        // already staged stays out — then the name group, its delimiters as written
        // (the rule cloned off the matched open token) around the name as read. The
        // span from `trigger_start` cannot answer this: for a language that does not
        // obey span tiling it is only what the reader describes for the stretch.
        let consumed = |cx: &ParseContext<'_, '_, LLL>, edge: TokenEdge| -> String {
            let mut text = String::from(
                cx.tokens.source_span_between(trigger, TokenEdge::Start, edge).content(),
            );
            if let Some(group) = &name_group {
                text.push_str(&group.rule().open);
                text.push_str(group.name_text());
                text.push_str(&group.rule().close);
            }
            text
        };
        // The condition quotes the terminator as written — its command name is this
        // spec's registration name, not a fixed spelling. Without a name group the
        // quote stops at the command word (`command_end`): the trigger's own
        // post-space is consumed with it and would read as a trailing blank inside
        // the quotes.
        let quoted_edge = match &name_group {
            Some(_) => TokenEdge::EndPastPostSpace,
            None => command_end,
        };
        let terminator = consumed(cx, quoted_edge);
        cx.recover(OrphanEnd::new(name, terminator), span.clone())?;
        // The node covers the trigger and, when one was read, the name group — several
        // tokens — so for a language with `OBEYS_SPAN_TILING = false` its content is
        // what was consumed (`consumed`, above), not the recorded span.
        let content = match LLL::OBEYS_SPAN_TILING {
            true => TextContent::Spanned(span.span()),
            false => TextContent::Owned(consumed(cx, TokenEdge::EndPastPostSpace).into()),
        };
        let id = cx
            .stage_node(
                NodeKind::chars(content),
                span.clone(),
                Arc::clone(&cx.state),
                vec![],
            )
            .map_err(|error| cx.staging_error(error, span))?;
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
    use crate::latexlike::{check_latexlike_tree_invariants, source_recomposer};
    use crate::node::NodeRef;
    use crate::recompose::TreeRecomposer;
    use crate::scopes::{Package, ScopeOp};
    use super::super::test_support::RelaxedLatexlike;
    use crate::state::{CommentOverrides, ParsingState, TokenRulesOverrides};
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

    #[test]
    fn dyn_environment_behavior_downcasts_to_its_concrete_type() {
        // The `Any` supertrait: the behavior a spec stores comes back type-erased
        // from the accessor a consumer actually uses — `EnvironmentSpec::behavior` —
        // and gives its concrete type back through dyn-to-`Any` upcasting.
        #[derive(Debug)]
        struct Custom {
            label: &'static str,
        }
        impl EnvironmentBehavior for Custom {}
        let spec: EnvironmentSpec =
            EnvironmentSpec::from_behavior(Arc::new(Custom { label: "custom" }));
        let any: &dyn Any = spec.behavior();
        let recovered = any.downcast_ref::<Custom>().expect("downcast to Custom");
        // The recovered reference answers the concrete value's own state.
        assert_eq!(recovered.label, "custom");
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
                    comments: CommentOverrides::disable(),
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
            ParsingState::lang_initial_with_packages([test_definitions()]).expect("seed state"),
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
        check_latexlike_tree_invariants(&result.tree);
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
        check_latexlike_tree_invariants(&result.tree);
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
                ParsingState::lang_initial_with_packages([math_envs]).expect("seed state"),
            )
        };

        let math = with_math_envs(Recovery::Strict)
            .parse("$\\begin{aligned}x\\end{aligned}$")
            .unwrap();
        check_latexlike_tree_invariants(&math.tree);
        assert!(math.diagnostics.is_empty());

        let text = with_math_envs(Recovery::Tolerant)
            .parse("\\begin{aligned}x\\end{aligned}")
            .unwrap();
        assert_eq!(messages(&text), ["unknown environment ‘aligned’"]);
    }

    // --- the `"_builtin"` seed: begin/end out of the box --------------------------------

    #[test]
    fn begin_and_end_dispatch_out_of_the_box() {
        // No extra packages: `\begin`/`\end` are `"_builtin"` entries; the environment
        // name is unknown, but the composition still parses the body to its
        // terminator under the tolerant fallback.
        let language = Language::new(
            LatexlikeDriver::new(Recovery::Tolerant),
            ParsingState::lang_initial().expect("seed state"),
        );
        let result = language.parse("\\begin{itemize}x\\end{itemize}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(messages(&result), ["unknown environment ‘itemize’"]);
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.environment_name(), Some("itemize"));
        assert_eq!(body_shapes(env), ["chars(x)"]);
    }

    #[test]
    fn unloading_builtin_removes_the_dispatch_pair() {
        let seed = ParsingState::<Latexlike>::lang_initial().expect("seed state")
            .derived(&ParsingStateDelta::new().scope_op(ScopeOp::Unload {
                name: "_builtin".into(),
            }))
            .unwrap();
        let language = Language::new(LatexlikeDriver::new(Recovery::Tolerant), seed);
        let result = language.parse("\\begin{itemize}").unwrap();
        let all = messages(&result);
        assert_eq!(all.len(), 1);
        assert!(all[0].contains("cannot resolve command ‘\\begin’"), "{}", all[0]);
        // `\begin` recovered as chars, `{itemize}` an ordinary group.
        assert_eq!(root_shapes(&result), ["chars(\\begin)", "group(Content { })"]);
    }

    // --- the dispatch pair's names -----------------------------------------------------

    /// The `\open{name} … \shut{name}` language: the opening command is named by its
    /// registration and the terminator by [`BeginSpec::new`]'s argument, so a renamed
    /// pair is definitions, not code. The seed `_builtin` pair is unloaded — nothing
    /// in this language answers to `\begin`.
    fn renamed_pair_language(recovery: Recovery) -> Language<Latexlike> {
        let mut package = Package::new("renamed");
        package.insert(CallableType::Macro, "open", BeginSpec::<Latexlike>::new("shut"));
        package.insert(CallableType::Macro, "shut", EndSpec::<Latexlike>::new());
        package.insert(CallableType::Environment, "itemize", env(vec![]));
        package.insert(
            CallableType::Environment,
            "verbatim",
            Arc::new(EnvironmentSpec::from_behavior(Arc::new(
                VerbatimBehavior::default(),
            ))),
        );
        let seed = ParsingState::<Latexlike>::lang_initial_with_packages([package])
            .expect("seed state")
            .derived(&ParsingStateDelta::new().scope_op(ScopeOp::Unload {
                name: "_builtin".into(),
            }))
            .unwrap();
        Language::new(LatexlikeDriver::new(recovery), seed)
    }

    /// Strict parse expected clean, then reemitted: the recorded begin/end spellings
    /// must give the input back byte for byte.
    fn parse_and_reemit(language: &Language<Latexlike>, input: &str) -> ParseResult<Latexlike> {
        let result = language.parse(input).unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(
            result.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            result.diagnostics
        );
        let out = TreeRecomposer::new(&mut source_recomposer())
            .recompose(&result.tree, ())
            .unwrap();
        assert_eq!(out, input);
        result
    }

    #[test]
    fn a_renamed_dispatch_pair_parses_environments() {
        assert_eq!(BeginSpec::<Latexlike>::new("shut").end_command_name(), "shut");

        let language = renamed_pair_language(Recovery::Strict);
        let result = parse_and_reemit(&language, "\\open{itemize} a \\shut{itemize}");
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.callable_type(), Some(CallableType::Environment));
        assert_eq!(env.environment_name(), Some("itemize"));
        assert_eq!(body_shapes(env), ["chars( a )"]);

        // The pair is this package's alone: `\begin` resolves to nothing here.
        let tolerant = renamed_pair_language(Recovery::Tolerant);
        let result = tolerant.parse("\\begin{itemize}\\end{itemize}").unwrap();
        let all = messages(&result);
        assert!(all[0].contains("cannot resolve command ‘\\begin’"), "{all:?}");
    }

    #[test]
    fn a_renamed_dispatch_pair_terminates_verbatim_bodies() {
        // The raw body reads up to the literal terminator composed from the
        // invocation's own spellings — the renamed command word included — and
        // reports it back as standard end facts, so the reemission is exact here too.
        let language = renamed_pair_language(Recovery::Strict);
        let result =
            parse_and_reemit(&language, "\\open{verbatim}\na % b \\x{\n\\shut{verbatim}");
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.environment_name(), Some("verbatim"));
        let body: Vec<_> = env.body().unwrap().iter().collect();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].chars(), Some("a % b \\x{\n"));
    }

    #[test]
    fn the_environment_conditions_quote_the_source_spelling() {
        // Neither condition spells a canonical `\begin`/`\end`: the malformed opening
        // quotes the command it was written with, and the orphan terminator quotes
        // its own consumed extent.
        let language = renamed_pair_language(Recovery::Tolerant);

        let result = language.parse("\\open x").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(
            messages(&result),
            ["malformed ‘\\open’: expected the environment's name group immediately \
              after the command"]
        );

        let result = language.parse("\\shut{itemize}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(
            messages(&result),
            ["orphan ‘\\shut{itemize}’: no environment ‘itemize’ is open here"]
        );
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
        check_latexlike_tree_invariants(&result.tree);
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
            ["orphan ‘\\end{itemize}’: no environment ‘itemize’ is open here"]
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
        // The quoted spelling stops at the command word: the trigger's post-space is
        // consumed with it (and staged), but reads as a stray blank inside quotes.
        assert_eq!(messages(&result), ["orphan ‘\\end’: no environment is open here"]);
        assert_eq!(root_shapes(&result), ["chars(\\end )", "chars(x)"]);
    }

    #[test]
    fn orphan_end_inside_a_group() {
        let result = parse_tolerant("{\\end{itemize}}");
        assert_eq!(
            messages(&result),
            ["orphan ‘\\end{itemize}’: no environment ‘itemize’ is open here"]
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
        let scratch: Arc<crate::source::Source> = Arc::new(crate::source::Source::new(""));
        EnvironmentInvocation {
            trigger_span: SourceSpan::new(&scratch, 0..0),
            name: "probe",
            name_span: SourceSpan::new(&scratch, 0..0),
            escape_char: '\\',
            name_group_open: "{",
            name_group_close: "}",
            end_command_name: "end",
        }
    }

    #[test]
    fn with_body_delta_overrides_any_behavior() {
        // On the declarative standard behavior… (the infallible impls answer
        // through the `Ok(...)` wrapping — their only use of the `Result`).
        let spec = EnvironmentSpec::<Latexlike>::new(vec![]);
        assert!(spec.behavior().body_state_delta(probe_invocation()).unwrap().is_none());
        let spec = spec.with_body_delta(ParsingStateDelta::new().mode(Mode::Math));
        let delta =
            spec.behavior().body_state_delta(probe_invocation()).unwrap().unwrap();
        assert_eq!(delta.mode, Some(Mode::Math));

        // …and wrapping a custom behavior.
        #[derive(Debug)]
        struct Custom;
        impl EnvironmentBehavior for Custom {
            fn body_state_delta(
                &self,
                _invocation: EnvironmentInvocation<'_>,
            ) -> Result<Option<ParsingStateDelta<Latexlike>>, ParseError> {
                Ok(Some(ParsingStateDelta::new().mode(Mode::Text)))
            }
        }
        let custom = EnvironmentSpec::from_behavior(Arc::new(Custom))
            .with_body_delta(ParsingStateDelta::new().mode(Mode::Math));
        let delta =
            custom.behavior().body_state_delta(probe_invocation()).unwrap().unwrap();
        assert_eq!(delta.mode, Some(Mode::Math));
    }

    #[test]
    fn a_failing_body_state_delta_aborts_under_any_policy() {
        // The hook-fallibility contract on `body_state_delta`: an Err ends the
        // parse even under tolerant recovery — the composition derives the body
        // state from the answer, so there is no recovery channel — and the
        // composition attaches the live traceback (the `\begin` invocation frame).
        #[derive(Debug)]
        struct Broken;
        impl EnvironmentBehavior for Broken {
            fn body_state_delta(
                &self,
                _invocation: EnvironmentInvocation<'_>,
            ) -> Result<Option<ParsingStateDelta<Latexlike>>, ParseError> {
                let scratch: Arc<crate::source::Source> =
                    Arc::new(crate::source::Source::new(""));
                Err(ParseError::new(
                    crate::error::HookFailed::new("body-delta table unavailable", None),
                    SourceSpan::new(&scratch, 0..0),
                ))
            }
        }

        let mut package = Package::new("broken");
        package.insert(
            CallableType::Environment,
            "bad",
            EnvironmentSpec::from_behavior(Arc::new(Broken)),
        );
        let language = Language::new(
            LatexlikeDriver::new(Recovery::Tolerant),
            ParsingState::lang_initial_with_packages([package]).expect("seed state"),
        );
        let error = language.parse("\\begin{bad}x\\end{bad}").unwrap_err();
        assert_eq!(error.identifier(), "core.hooks.hook-failed");
        assert_eq!(
            error.message(),
            "extension hook reported a failure: body-delta table unavailable"
        );
        // The `\begin` dispatch frame is live at the consultation site — the only
        // frame, as `bad` declares no arguments.
        assert_eq!(error.frames().len(), 1);
        assert_eq!(error.frames()[0].title(), "macro ‘\\begin’");
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
            Arc::new(crate::spec::StdCallableSpec { arguments: vec![brace_arg()], ..Default::default() }),
        );
        let language = Language::new(
            LatexlikeDriver::new(Recovery::Strict),
            ParsingState::lang_initial_with_packages([package]).expect("seed state"),
        );
        let result = language.parse("\\begin{gen}{a}x\\end{gen}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
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
        // The raw chars node records the features-disabled verbatim state.
        assert!(!body[0].parsing_state().rules().commands_enabled());
        assert!(!body[0].parsing_state().rules().comments_enabled());

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
    // --- a latexlike-family language that does not obey span tiling (PLAN §1.5 R7) ----

    /// A `Language` over [`RelaxedLatexlike`] with `itemize` defined.
    fn relaxed_language() -> Language<RelaxedLatexlike> {
        let mut package: Package<RelaxedLatexlike> = Package::new("relaxed-envs");
        package
            .define_environment("itemize", core::iter::empty::<&str>())
            .expect("no argument codes");
        Language::new(
            LatexlikeDriver::new(Recovery::Tolerant),
            ParsingState::lang_initial_with_packages([Arc::new(package)]).expect("seed state"),
        )
    }

    /// The environment's name is read from a rigid name group — several `Char` tokens
    /// — and drives the spec lookup, the node's `name` and the diagnostics. For a
    /// language with `OBEYS_SPAN_TILING = false` it therefore comes from the characters
    /// as read, not from the name group's span; a wrong name would show up here as an
    /// unresolved environment.
    #[test]
    fn an_environment_name_is_read_exactly_where_the_language_does_not_obey_span_tiling() {
        // Over the standard reader the described span happens to be the exact range,
        // so what this pins is the accumulation path: the name the lookup and the node
        // see is the one `read_name_chars` collected character by character. A reader
        // whose described span says something else is the scripted reader's business.
        let input = r"\begin{itemize}x\end{itemize}";

        let mut package: Package<Latexlike> = Package::new("envs");
        package.define_environment("itemize", core::iter::empty::<&str>()).unwrap();
        let tiled = Language::new(
            LatexlikeDriver::new(Recovery::Tolerant),
            ParsingState::lang_initial_with_packages([Arc::new(package)]).expect("seed state"),
        )
        .parse(input)
        .expect("the parse runs");
        let relaxed = relaxed_language().parse(input).expect("the parse runs");

        assert!(tiled.diagnostics.is_empty(), "{:?}", tiled.diagnostics);
        assert!(
            relaxed.diagnostics.is_empty(),
            "the lookup found the environment: {:?}",
            relaxed.diagnostics
        );

        let tiled_env = tiled.tree.root().child(0).expect("the environment");
        let relaxed_env = relaxed.tree.root().child(0).expect("the environment");
        assert_eq!(tiled_env.name(), Some("itemize"));
        assert_eq!(relaxed_env.name(), Some("itemize"));
        assert_eq!(tiled_env.span().range(), relaxed_env.span().range());
        assert_eq!(
            tiled_env.children().len(),
            relaxed_env.children().len(),
            "the two environments differ in shape"
        );
        crate::node::validate_tree(&relaxed.tree).expect("the all-trees law holds");
        crate::latexlike::check_latexlike_tree_invariants(&relaxed.tree);
    }

    /// The orphan-`\end` recovery node covers the trigger and its name group — several
    /// tokens — so for a language with `OBEYS_SPAN_TILING = false` its content is the
    /// text of what was consumed, not the recorded span (which is only what the reader
    /// describes for the stretch). The diagnostic quotes the same text.
    #[test]
    fn the_orphan_end_recovery_owns_its_text_where_the_language_does_not_obey_span_tiling() {
        let relaxed = relaxed_language();

        for (input, recovered, quoted) in [
            ("a\\end{itemize}b", "\\end{itemize}", "\\end{itemize}"),
            // Without a name group the quote stops at the command word.
            ("\\end x", "\\end ", "\\end"),
        ] {
            // The tiled parse of the same input, for comparison.
            let tiled = parse_tolerant(input);
            let index = usize::from(input.starts_with('a'));
            let tiled_node = tiled.tree.root().child(index).expect("the recovery node");
            assert_eq!(tiled_node.chars(), Some(recovered));
            assert!(
                matches!(
                    tiled_node.kind(),
                    NodeKind::Chars { content: TextContent::Spanned(_), .. }
                ),
                "a tiled parse records the recovered extent as a span"
            );

            let result = relaxed.parse(input).expect("tolerant recovery");
            crate::latexlike::check_latexlike_tree_invariants(&result.tree);
            // The condition quotes the terminator as written — assembled the same way,
            // so the diagnostic names what was actually read.
            let rendered: Vec<String> =
                result.diagnostics.iter().map(|d| d.message().to_string()).collect();
            assert_eq!(rendered.len(), 1, "{rendered:?}");
            assert!(
                rendered[0].contains(&alloc::format!("orphan ‘{quoted}’")),
                "the diagnostic quotes the terminator as read: {rendered:?}"
            );
            let node = result.tree.root().child(index).expect("the recovery node");
            assert_eq!(node.chars(), Some(recovered), "the recovered text differs from {input:?}");
            assert!(
                matches!(node.kind(), NodeKind::Chars { content: TextContent::Owned(_), .. }),
                "a relaxed parse records the recovered extent as text, got {:?}",
                node.kind()
            );
            crate::node::validate_tree(&result.tree).expect("the all-trees law holds");
        }
    }
}
