//! Defining callables: callable specs, definition providers and scopes, and command
//! resolution — the author-side vocabulary of the core.
//!
//! - **Callable specs** — a [`CallableSpec`] records *callable behavior*, de-keyed
//!   from the name and invocation form it is registered under; [`StdCallableSpec`]
//!   is the standard implementation, declaring its arguments as [`ArgumentSpec`]s.
//!   The [`ArgumentParser`](crate::core::constructs::ArgumentParser) *parsing*
//!   contract lives in [`constructs`](crate::core::constructs), beside the shipped
//!   argument-parser implementations (`ArgumentSpec` holding an
//!   `Arc<dyn ArgumentParser>` is an accepted cross-boundary reference).
//! - **Providers and scopes** — [`SpecsProvider`] is the stack-entry contract;
//!   [`Package`] (immutable) and [`Scope`] (mutable by replacement) are the standard
//!   implementations; [`FallbackProvider`] expresses the unknown-callable policy;
//!   [`ScopeStack`] searches innermost-first (lexical shadowing). Definitions are
//!   reshaped mid-parse through [`ScopeOp`]s / [`DefinitionOp`]s carried by parsing
//!   state deltas. Registration takes the crate's sealed Arc-removal conversions:
//!   [`IntoCallableSpec`] (specs into [`Package::insert`] and siblings),
//!   [`IntoArgumentParser`] (parsers into [`ArgumentSpec::new`] and
//!   [`new_unnamed`](ArgumentSpec::new_unnamed)), and
//!   [`IntoSpecsProvider`] (packages/providers into
//!   [`ParsingState::lang_initial_with_packages`](crate::core::ParsingState::lang_initial_with_packages)).
//! - **Command resolution** — [`resolve_command_in_scopes`] is the standard
//!   resolution body (build the query, consult the scope stack, map the outcome —
//!   a clean miss carries the searched providers plus a did-you-mean detail),
//!   packaged as the [`ScopesCommandResolver`] strategy for
//!   [`StdParseDriver`](crate::core::StdParseDriver);
//!   [`CommandResolution`] (with [`ResolvedCallable`]) is the outcome vocabulary;
//!   [`CallableQuery`], [`CallableSyntax`], and [`SearchedProviders`] describe the
//!   lookup. [`check_provider_commands_shadowed_by_escape`] is the
//!   parse-initialization registration-sanity check (warning condition
//!   [`ProviderCommandsShadowedByEscape`]).
//!
//! The run-side machinery that consumes these definitions — state, tokens, engine —
//! is the [`core`](crate::core) hub.

pub use crate::engine::{
    resolve_command_in_scopes, CommandResolution, ResolvedCallable, ScopesCommandResolver,
};
pub use crate::scopes::{
    check_provider_commands_shadowed_by_escape, CallableDefinedAsError, CallableQuery,
    CallableSyntax, DefinitionOp, ErrorCallableSpec, FallbackProvider, IntoSpecsProvider,
    Package, ProviderCommandsShadowedByEscape, ProviderError, Scope, ScopeOp,
    ScopeOpError, ScopeStack, ScopeStackError, SearchedProviders, SpecsProvider,
    SymbolEntry,
};
pub use crate::spec::{
    ArgumentSpec, CallableSpec, IntoArgumentParser, IntoCallableSpec, StdCallableSpec,
};
