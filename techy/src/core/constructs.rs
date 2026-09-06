//! Construct parsers: the [`ConstructParser`] contract, and the standard parsers
//! and argument parsers written against it.
//!
//! Every construct is parsed by a [`ConstructParser`] implementation that reads
//! tokens and stages nodes through one [`ParseContext`]. The standard shapes ship
//! here: the root parse ([`RootNodesParser`], which the parse entry point runs
//! directly, above every descent), the content loop that dispatches on each token
//! ([`NodesParser`], with its stop conditions [`StopSpec`] and [`StopCause`]),
//! groups ([`GroupParser`], with the [`ChildStateSpec`] policy deciding the state a
//! child construct is parsed under), callable invocations
//! ([`StdInvocationParser`], which takes the resolved [`Invocation`] as its input),
//! environment bodies ([`EnvironmentBodyParser`]), and verbatim material
//! ([`VerbatimBodyParser`]).
//!
//! An argument is a parser too: [`ArgumentParser`] returns
//! [`ParsedArgumentNodes`], and the standard argument forms — delimited group,
//! optional group, chars group, literal marker, verbatim, embellishments, tack-on
//! fields — are shipped as ordinary parsers you configure with the group types and
//! rules they should use. Each parser's diagnostic conditions are documented next
//! to the parser that raises them.
//!
//! [Writing a construct parser](crate::guide::construct_parsers) introduces the job
//! from scratch, and [the parsing model](crate::guide::parsing_model) shows where
//! these parsers sit in a whole parse.
//!
//! Three contracts hold for everything in this module.
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

// Condition types stay next to the parser that raises them, with one deliberate
// exception: `InvalidSourceReferenceArgument` is defined beside the two conditions
// of `ParseContext::attach_source_reference` and raised by the include-like specs
// that read a source reference out of an argument, so that all three failures of an
// inclusion read the same wherever they come from.

pub use crate::constructs::{
    parse_declared_arguments, peek_adjacent_argument, read_rigid_name_group,
    scan_argument_noise, stage_pre_space, verbatim_state_delta, ArgumentNoise, AttachedSourceOutcome, CharsGroupArgumentParser,
    ChildStateSpec, CommandResolutionFailed, ConstructParser, ConstructParserResult,
    DescentLimitApproaching, DescentLimitExceeded,
    EmbellishmentsArgumentParser, EnvironmentBeginSyntaxData, EnvironmentBody,
    EnvironmentBodyParser, EnvironmentTerminatorMismatch, EnvironmentTerminatorSyntaxData,
    ExpectedExpressionArgument, ExpectedVerbatimDelimiter,
    ExpressionCallableRequiresContent, ExpressionParser, FromInvocation,
    GroupAfterEffectsFn, GroupArgumentParser, GroupChildState, GroupParser,
    ImplementationError, InvalidReferenceReason, InvalidSourceReferenceArgument, Invocation,
    InvocationChildState,
    MalformedEnvironmentTerminator, MarkerArgumentParser, MissingEnvironmentTerminator,
    MissingMandatoryArgument, MissingTerminatorFound, NameGroup, NoSourceResolver,
    NodesOutcome, NodesParser,
    OptionalGroupArgumentParser, ParseContext, RepeatedTackOnField, RootNodesParser,
    ScopeOpFailed,
    StdInvocationParser, StopCause, StopSpec, StrayGroupClose, TackOnFieldsArgumentParser,
    TokenStopCondition, TokenStopKind, UnclosedGroup, UnclosedGroupFound,
    UnresolvableCommand, UnresolvableSourceReference, UnterminatedVerbatim,
    UnusableRecoveryToken, UnusableRecoveryTokenKind, VerbatimArgumentParser,
    VerbatimBodyParser, VerbatimBodyTerminator,
};
pub use crate::spec::{ArgumentParser, ParsedArgumentNodes};
