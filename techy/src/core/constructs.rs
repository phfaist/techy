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

pub use crate::constructs::{
    parse_declared_arguments, read_rigid_name_group, scan_argument_noise, stage_pre_space,
    verbatim_state_delta, ArgumentNoise, CharsGroupArgumentParser, ChildStateSpec,
    CommandResolutionFailed, ConstructParser, ConstructParserResult,
    EmbellishmentsArgumentParser, EnvironmentBeginSyntaxData, EnvironmentBody,
    EnvironmentBodyParser, EnvironmentTerminatorMismatch, EnvironmentTerminatorSyntaxData,
    ExpectedExpressionArgument, ExpectedVerbatimDelimiter,
    ExpressionCallableRequiresContent, ExpressionParser, FromInvocation,
    GroupArgumentParser, GroupChildState, GroupParser, ImplementationError, Invocation,
    InvocationChildState,
    MalformedEnvironmentTerminator, MarkerArgumentParser, MissingEnvironmentTerminator,
    MissingMandatoryArgument, MissingTerminatorFound, NameGroup, NodesOutcome, NodesParser,
    OptionalGroupArgumentParser, ParseContext, RepeatedTackOnField, ScopeOpFailed,
    StdInvocationParser, StopCause, StopSpec, StrayGroupClose, TackOnFieldsArgumentParser,
    TokenStopCondition, TokenStopKind, UnclosedGroup, UnclosedGroupFound,
    UnresolvableCommand, UnterminatedVerbatim, UnusableRecoveryToken,
    UnusableRecoveryTokenKind, VerbatimArgumentParser, VerbatimBodyParser,
};
pub use crate::spec::{ArgumentParser, ParsedArgumentNodes};
