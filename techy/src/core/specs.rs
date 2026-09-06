//! Defining callables: telling techy what `\cite` or `\begin{equation}` means.
//!
//! techy ships almost no definitions of its own, so this is the module almost every
//! program touches. The guide chapter
//! [Defining macros, environments, and specials](crate::guide::specs) walks through
//! the whole task; this page is the map of the API.
//!
//! # A definition is a spec stored under a name
//!
//! A [`CallableSpec`] describes one callable's *behavior*: the arguments it takes and
//! how its invocation is parsed. It holds no name and no invocation form of its own —
//! the name under which a callable is invoked belongs to the provider that stores the
//! spec, so one spec value may be stored under several names (`\emph` and `\textit`
//! can share one), and a single spec can answer for every unknown callable.
//!
//! [`StdCallableSpec`] is the standard implementation: a list of [`ArgumentSpec`]s
//! and nothing more. Each argument spec names the
//! [`ArgumentParser`](crate::core::constructs::ArgumentParser) that recognizes that
//! one argument; the shipped parsers (mandatory group, optional group, marker,
//! expression, verbatim) are in [`constructs`](crate::core::constructs). Building the
//! list by hand is rarely necessary: the latexlike preset builds it from short
//! argument codes — `"o"` for an optional `[…]` argument, `"m"` for a mandatory one —
//! documented with the full code table in
//! [`argument_specs`](crate::latexlike::argument_specs), and used directly by the
//! one-liners [`Package::define_macro`] and [`Package::define_environment`].
//!
//! # Where definitions are stored
//!
//! A store of definitions is a [`SpecsProvider`]. Three implementations ship:
//!
//! - [`Package`] — an immutable collection, built once with
//!   [`insert`](Package::insert) and loaded wholesale into the initial parsing state
//!   through
//!   [`ParsingState::lang_initial_with_packages`](crate::core::ParsingState::lang_initial_with_packages).
//!   This is where definitions normally go.
//! - [`Scope`] — the target of definitions made *during* a parse. Updates are made by
//!   replacement rather than in place, which is what makes a definition made inside a
//!   group disappear again when the group ends.
//! - [`FallbackProvider`] — answers *any* name of the callable types registered with
//!   it, so an unknown macro still resolves to a spec instead of failing.
//!
//! [`ErrorCallableSpec`] is the definition that means "invoking this name is an
//! error": it records a [`CallableDefinedAsError`] diagnostic and recovers. Storing it
//! over an existing definition is how a name is taken away again.
//!
//! # How a name is looked up
//!
//! The providers in effect are a [`ScopeStack`], stored in the parsing state and
//! searched **innermost first** — the provider pushed last answers first. So loading a
//! package that redefines a name shadows the definition below it, and a fallback
//! provider at the bottom answers only what nothing above it defines. When the whole
//! stack has been searched without a hit, the callable is unresolvable;
//! [`SearchedProviders`] renders the providers that were searched for the diagnostic.
//!
//! [`resolve_command_in_scopes`] is the standard lookup: it builds a
//! [`CallableQuery`] (the callable type, the name, and the [`CallableSyntax`] the
//! invocation was written in), searches the stack, and reports a
//! [`CommandResolution`] — a [`ResolvedCallable`] or a miss whose detail names the
//! searched providers and suggests near-misses. [`ScopesCommandResolver`] is that
//! lookup packaged as the resolution strategy of
//! [`StdParseDriver`](crate::core::StdParseDriver).
//!
//! A common registration mistake is to register a command name *with* its escape
//! character (`"\\greet"` instead of `"greet"`), which makes the definition
//! unreachable. [`check_provider_commands_shadowed_by_escape`] warns about it at parse
//! initialization ([`ProviderCommandsShadowedByEscape`]).
//!
//! # Changing definitions during a parse
//!
//! A parsing state delta carries [`ScopeOp`]s: push, unload or replace a provider, or
//! define and remove single names in a provider by name. A provider receives the
//! definition changes addressed to it as [`DefinitionOp`]s, through
//! [`with_definitions`](SpecsProvider::with_definitions). Failures are reported as
//! [`ScopeOpError`], [`ScopeStackError`] and [`ProviderError`].
//!
//! # Passing specs and providers by value
//!
//! Registration calls take their arguments through sealed conversions, so that a value
//! passes by value and an already-shared `Arc` passes through unchanged:
//! [`IntoCallableSpec`] for specs given to [`Package::insert`] and its siblings,
//! [`IntoArgumentParser`] for parsers given to [`ArgumentSpec::new`] and
//! [`new_unnamed`](ArgumentSpec::new_unnamed), and [`IntoSpecsProvider`] for providers
//! given to
//! [`lang_initial_with_packages`](crate::core::ParsingState::lang_initial_with_packages).
//!
//! # Serializing definitions
//!
//! A package built with [`Package::new_shared`] can stamp its specs with a
//! [`SpecProvenance`] recording which provider defined the spec and under which
//! [`DefinitionKey`]. That stamp is what lets a spec be serialized as a reference to
//! its definition rather than described in full — see [`serialize`](crate::serialize).
//!
//! The machinery that consumes these definitions while parsing — parsing state,
//! tokens, the engine — is in [`core`](crate::core).

pub use crate::engine::{
    resolve_command_in_scopes, CommandResolution, ResolvedCallable, ScopesCommandResolver,
};
pub use crate::scopes::{
    check_provider_commands_shadowed_by_escape, CallableDefinedAsError, CallableQuery,
    CallableSyntax, DefinitionKey, DefinitionOp, ErrorCallableSpec, FallbackProvider, IntoSpecsProvider,
    Package, ProviderCommandsShadowedByEscape, ProviderError, Scope, ScopeOp,
    ScopeOpError, ScopeStack, ScopeStackError, SearchedProviders, SpecProvenance, SpecsProvider,
    SymbolEntry,
};
pub use crate::spec::{
    ArgumentSpec, CallableSpec, IntoArgumentParser, IntoCallableSpec, StdCallableSpec,
};
