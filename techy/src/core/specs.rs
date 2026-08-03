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
//!   state deltas.
//! - **Command resolution** — [`CommandResolution`] (with [`ResolvedCallable`]) is
//!   the outcome vocabulary of resolving a command token against the scope stack;
//!   [`CallableQuery`], [`CallableSyntax`], and [`SearchedProviders`] describe the
//!   lookup.
//!
//! The run-side machinery that consumes these definitions — state, tokens, engine —
//! is the [`core`](crate::core) hub.

pub use crate::engine::{CommandResolution, ResolvedCallable};
pub use crate::scopes::{
    CallableDefinedAsError, CallableQuery, CallableSyntax, DefinitionOp, ErrorCallableSpec,
    FallbackProvider, Package, ProviderError, Scope, ScopeOp, ScopeOpError, ScopeStack,
    ScopeStackError, SearchedProviders, SpecsProvider, SymbolEntry,
};
pub use crate::spec::{ArgumentSpec, CallableSpec, StdCallableSpec};
