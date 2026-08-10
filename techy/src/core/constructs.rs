//! Construct parsing: the [`ConstructParser`] contract and everything written
//! against it.
//!
//! Every construct is parsed by a [`ConstructParser`] implementation reading tokens
//! and staging nodes through one [`ParseContext`]: the main content loop
//! ([`NodesParser`], with its stop conditions [`StopSpec`]/[`StopCause`]), groups
//! ([`GroupParser`], the [`ChildStateSpec`] descent policy), callable invocations
//! ([`StdInvocationParser`], with [`Invocation`] as the parsers' input bundle),
//! arguments, environment bodies ([`EnvironmentBodyParser`]), and verbatim.
//!
//! The argument-parsing contract lives here beside its implementations: an argument
//! **is a parser** — [`ArgumentParser`], returning [`ParsedArgumentNodes`] — and the
//! standard forms (delimited group, optional group, chars group, literal marker,
//! verbatim, embellishments, tack-on fields) are shipped as ordinary construct
//! parsers, parameterized by group types and rules.
//!
//! Diagnostic condition types stay producer-side: each parser's conditions are
//! defined here, next to the parser that raises them.
//!
//! Three module-level contracts hold for everything in this module:
//!
//! # The two-tier ownership model
//!
//! Construct parsers are **temporaries** (tier 2): each is constructed with its
//! per-use configuration where it is needed, keeps working state in its own fields
//! ([`ConstructParser::parse`] takes `&mut self`), may freely borrow, and is dropped
//! when its construct's parse ends — construct parsers are never stored in specs.
//! *Stored* behavior objects (tier 1 — specs and [`ArgumentParser`]s) are
//! `Arc`-shared, immutable, `Send + Sync` by contract, and receive every per-use
//! input as arguments. Closures (such as stop predicates) are thereby confined to
//! tier 2; specs stay data.
//!
//! # State threading: the caller applies deltas
//!
//! [`ParseContext::state`] is the parser's **input** parsing state — the caller sets
//! it. A parser that parses child content under a modified state (a group interior,
//! an argument extent, a slot body) derives the child state and scopes it
//! structurally ([`ParseContext::with_parsing_state`] and its siblings): the outer
//! state is restored when the descent returns, because the caller still holds it.
//! The optional [`ParsingStateDelta`](crate::core::ParsingStateDelta) in
//! [`ConstructParser::parse`]'s return value is exclusively the construct's
//! **after-effect for the caller** (as with `\newcommand`, whose definition must
//! outlive the construct): the parser never applies it itself — deltas are plain
//! values, and the caller decides whether and where they apply.
//!
//! # Errors
//!
//! `Err` means **abort**: nobody continues past an `Err` from a construct parser.
//! Recovery from problems in the source happens *before* returning, at the
//! detection site — every detected problem is reported through
//! [`ParseContext::recover`], which applies the driver's recovery policy — and
//! abnormal endings of sub-parses travel as data ([`StopCause`]).

pub use crate::constructs::{
    parse_declared_arguments, read_rigid_name_group, scan_argument_noise, stage_pre_space,
    verbatim_state_delta, ArgumentNoise, AttachedSourceOutcome, CharsGroupArgumentParser,
    ChildStateSpec, CommandResolutionFailed, ConstructParser, ConstructParserResult,
    DescentLimitApproaching, DescentLimitExceeded,
    EmbellishmentsArgumentParser, EnvironmentBeginSyntaxData, EnvironmentBody,
    EnvironmentBodyParser, EnvironmentTerminatorMismatch, EnvironmentTerminatorSyntaxData,
    ExpectedExpressionArgument, ExpectedVerbatimDelimiter,
    ExpressionCallableRequiresContent, ExpressionParser, FromInvocation,
    GroupArgumentParser, GroupChildState, GroupParser, ImplementationError, Invocation,
    InvocationChildState,
    MalformedEnvironmentTerminator, MarkerArgumentParser, MissingEnvironmentTerminator,
    MissingMandatoryArgument, MissingTerminatorFound, NameGroup, NoSourceResolver,
    NodesOutcome, NodesParser,
    OptionalGroupArgumentParser, ParseContext, RepeatedTackOnField, ScopeOpFailed,
    StdInvocationParser, StopCause, StopSpec, StrayGroupClose, TackOnFieldsArgumentParser,
    TokenStopCondition, TokenStopKind, UnclosedGroup, UnclosedGroupFound,
    UnresolvableCommand, UnresolvableSourceReference, UnterminatedVerbatim,
    UnusableRecoveryToken, UnusableRecoveryTokenKind, VerbatimArgumentParser,
    VerbatimBodyParser,
};
pub use crate::spec::{ArgumentParser, ParsedArgumentNodes};
