//! The scope stack: how callable names resolve to [`CallableSpec`]s.
//!
//! The scope-stack design:
//!
//! - [`SpecsProvider`] is the stack-entry contract: a named, fallible resolver
//!   ([`retrieve_spec`](SpecsProvider::retrieve_spec)) that may also participate in the
//!   tokenizer's specials scan ([`scan_specials`](SpecsProvider::scan_specials) +
//!   [`specials_trigger_chars`](SpecsProvider::specials_trigger_chars)) and accept
//!   functional definition updates ([`with_definitions`](SpecsProvider::with_definitions)).
//! - [`Package`] is the immutable standard implementation: built once, loaded wholesale,
//!   with optional per-mode visibility and a data-driven specials table.
//! - [`Scope`] is the mutable-by-replacement standard implementation — the target of
//!   definition ops, updated copy-on-write through `with_definitions`.
//! - [`FallbackProvider`] answers *any* name of its registered callable types — the
//!   unknown-callable policy expressed as an ordinary bottom-of-stack provider.
//! - [`ErrorCallableSpec`] makes "defined to be an error" an ordinary definition: its
//!   invocation parser records a diagnostic ([`CallableDefinedAsError`]) and recovers.
//! - [`ScopeStack`] is the ordered stack stored in the parsing state
//!   ([`StateData::scopes`](crate::state::StateData)), searched **innermost
//!   (last-pushed) first** — lexical shadowing, no `ConflictStrategy`. The stack itself
//!   is *not* a provider (stacks do not nest — this removes the
//!   nested-fallback-preemption hazard instead of re-mitigating it), and it is
//!   visibility-blind: mode visibility is each provider's own business.
//!
//! # Extending definitions mid-parse
//!
//! Deltas carry [`ScopeOp`]s ([`ParsingStateDelta::scope_ops`](crate::state::ParsingStateDelta)):
//! stack-shape ops (push / unload / replace / replace-stack) and definition ops routed to
//! a named provider's `with_definitions` ([`DefinitionOp`]). Ops carry `Arc`s directly —
//! the core has no name→package registry (a by-name `load("amsmath")` belongs to preset
//! driver helpers, which construct the `Arc` when *building* the delta). Scoped reversion
//! stays structural: outer states hold the old `Arc`s, so both a pushed provider and a
//! copy-on-write update of an outer scope revert when the group ends — lexical scoping
//! falls out of state immutability with zero per-group cost. Op failures surface through
//! the fallible [`ParsingState::derived`](crate::state::ParsingState::derived) (see
//! [`ScopeOpError`] and [`DeriveError`](crate::state::DeriveError)).
//!
//! # Specials across providers
//!
//! [`ScopeStack::scan_specials`] folds over every provider innermost-first: the
//! **longest match wins; ties go innermost** — exact pylatexenc `test_for_specials`
//! parity (`---` beats `--` wherever they are defined), and since equal-length matches
//! at one position are the *same spelling*, the tie rule is what makes shadowing a
//! redefined special work. A provider's scan error aborts the fold and propagates.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::any::Any;
use core::fmt;

use hashbrown::HashMap;

use crate::constructs::{
    ConstructParser, ConstructParserResult, ImplementationError, Invocation, ParseContext,
};
use crate::error::{Diagnostic, DiagnosticInfo, Diagnostics};
use crate::node::{BuildId, NodeKind};
use crate::serialize::SerializableObject;
use crate::source::{Source, SourceSpan, Span};
use crate::spec::{CallableSpec, IntoCallableSpec};
use crate::state::{
    ClosedVocabulary, FeaturePresence, Lang, LangFeatures, LangHasScopes, ParsingState,
    ParsingStateDelta,
};
use crate::token::{
    SpecialsMatch, SpecialsScanError, TokenErrorKind, TokenKindView, TriggerChars,
};

mod provenance;

pub use provenance::{DefinitionKey, SpecProvenance};

/// The syntax through which a callable invocation was recognized.
///
/// Carried on a [`CallableQuery`] so resolvers can disambiguate between coexisting
/// command syntaxes — with several [`CommandRule`](crate::token::CommandRule)s in scope,
/// `\foo` and `#foo` both tokenize as `Command { name: "foo" }`, and the escape character
/// is *not* recoverable from the token alone (a resolver has no access to the source
/// content behind the token's span).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallableSyntax {
    /// A command token: escape character + name (`\foo`).
    Command {
        /// The escape character that fired ([`CommandRule::escape_char`](crate::token::CommandRule)).
        escape_char: char,
    },
    /// A specials trigger (queried from inside a `Lang::scan_specials` implementation,
    /// before any token exists).
    Specials,
    /// Anything else — synthesized invocations, preset-defined resolution paths.
    Other,
}

/// A callable resolution request: everything a [`SpecsProvider`] may dispatch on, minus
/// the parsing state (passed alongside).
///
/// `token_kind` is the **view** of the triggering token, present when resolution
/// happens for an already-scanned token (the nodes parser resolving a `Command`) and
/// `None` where no token exists yet (specials scan, synthesized invocations). The view
/// is what a provider can know about the token: a provider has no reader, so it never
/// sees spans or pre-space. `name` and `syntax` repeat the `Command` facts for
/// providers that ignore the view — and plain data providers ignore everything but
/// `callable_type` and `name`.
pub struct CallableQuery<'a, L: Lang> {
    /// The invocation form being resolved (e.g. the preset's macro form).
    pub callable_type: L::CallableTypeId,
    /// The name to resolve. Providers store *normalized* names; normalization is the
    /// caller's (preset's) business — the core matches exactly.
    pub name: &'a str,
    /// The syntax context of the invocation.
    pub syntax: CallableSyntax,
    /// The view of the triggering token, when one exists.
    pub token_kind: Option<TokenKindView<'a, L>>,
}

impl<'a, L: Lang> CallableQuery<'a, L> {
    /// A query without token context.
    pub fn new(
        callable_type: L::CallableTypeId,
        name: &'a str,
        syntax: CallableSyntax,
    ) -> CallableQuery<'a, L> {
        CallableQuery { callable_type, name, syntax, token_kind: None }
    }

    /// Attach the view of the triggering token.
    pub fn with_token_kind(mut self, token_kind: TokenKindView<'a, L>) -> CallableQuery<'a, L> {
        self.token_kind = Some(token_kind);
        self
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug` although only references and
// plain data are stored.

impl<L: Lang> Clone for CallableQuery<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang> Copy for CallableQuery<'_, L> {}

impl<L: Lang> fmt::Debug for CallableQuery<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallableQuery")
            .field("callable_type", &self.callable_type)
            .field("name", &self.name)
            .field("syntax", &self.syntax)
            .field("token_kind", &self.token_kind)
            .finish()
    }
}

// --- provider failure shapes ---------------------------------------------------------

/// A [`SpecsProvider`] operation failed — the provider-level error of
/// [`retrieve_spec`](SpecsProvider::retrieve_spec) and
/// [`with_definitions`](SpecsProvider::with_definitions). Structured for testability; custom providers report anything beyond the
/// named cases through [`Failed`](ProviderError::Failed).
///
/// A provider error is an *operational* failure (misconfiguration, extension bug, backing
/// I/O), never a source condition: source-shaped failures of the specials scan travel as
/// [`TokenError`](crate::token::TokenError)s instead, and "this name is an error" is
/// expressed as an ordinary [`ErrorCallableSpec`] definition.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProviderError {
    /// The provider does not accept definition ops (the default
    /// [`with_definitions`](SpecsProvider::with_definitions) — e.g. a [`Package`]).
    NotMutable,
    /// An op referenced a name the provider does not define
    /// ([`DefinitionOp::Remove`] of an absent definition).
    UndefinedName {
        /// The name the op referenced.
        name: Box<str>,
    },
    /// A provider-specific failure, rendered as a message.
    Failed(Box<str>),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::NotMutable => {
                write!(f, "the provider does not accept definition ops")
            }
            ProviderError::UndefinedName { name } => {
                write!(f, "no definition named ‘{name}’ in this provider")
            }
            ProviderError::Failed(message) => write!(f, "{message}"),
        }
    }
}

/// A provider *within a stack* failed: the [`ProviderError`] plus which provider it was.
/// The `Err` of [`ScopeStack::retrieve_spec`] (a failing provider aborts the resolution
/// fold), and the payload of [`ScopeOpError::Provider`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeStackError {
    /// The name of the provider that failed.
    pub provider: Box<str>,
    /// What went wrong.
    pub error: ProviderError,
}

impl fmt::Display for ScopeStackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "provider ‘{}’: {}", self.provider, self.error)
    }
}

impl core::error::Error for ProviderError {}

impl core::error::Error for ScopeStackError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// One [`ScopeOp`] failed while a delta was applied — the per-op failure record
/// collected by the fallible [`ParsingState::derived`](crate::state::ParsingState::derived)
/// into a [`DeriveError`](crate::state::DeriveError). Mechanical, not classified: the *caller* decides what a failure means —
/// the in-parse derivation path treats it as a recoverable condition, reported
/// through the recovery entry point
/// ([`ScopeOpFailed`](crate::constructs::ScopeOpFailed)); an embedder applying a delta
/// out of parse treats it as its own input error.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScopeOpError {
    /// The op targeted a provider name not on the stack ([`ScopeOp::Unload`],
    /// [`ScopeOp::Replace`], [`ScopeOp::Remove`]; [`ScopeOp::Define`] instead creates
    /// the scope lazily).
    UnknownProvider {
        /// The name the op targeted.
        name: Box<str>,
    },
    /// The targeted provider rejected or failed the op.
    Provider(ScopeStackError),
    /// The stack belongs to a language that declares the scope stack absent
    /// ([`Lang::Features`]): no scope operation can apply. Reachable only by calling
    /// [`ScopeStack::apply_op`] directly on such a stack — a state delta can never
    /// carry scope ops for one, because the delta's scope-op list is stored through
    /// the same declaration and holds nothing for it.
    ScopesAbsent,
}

impl fmt::Display for ScopeOpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScopeOpError::UnknownProvider { name } => {
                write!(f, "no provider named ‘{name}’ on the scope stack")
            }
            ScopeOpError::Provider(error) => fmt::Display::fmt(error, f),
            ScopeOpError::ScopesAbsent => write!(
                f,
                "the language declares the scope stack absent; no scope operation can apply"
            ),
        }
    }
}

impl core::error::Error for ScopeOpError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            ScopeOpError::UnknownProvider { .. } => None,
            ScopeOpError::Provider(error) => Some(error),
            ScopeOpError::ScopesAbsent => None,
        }
    }
}

// --- ops -------------------------------------------------------------------------------

/// A definition operation as a [`SpecsProvider`] receives it
/// ([`with_definitions`](SpecsProvider::with_definitions)): the provider *is* the scope,
/// so — unlike the delta-level [`ScopeOp`] — no target name is carried.
pub enum DefinitionOp<L: Lang> {
    /// Define `name` under the invocation form `callable_type` (replacing any existing
    /// definition under that key — shadowing within one provider is replacement).
    Define {
        /// The invocation form the definition answers.
        callable_type: L::CallableTypeId,
        /// The (normalized) name being defined.
        name: Box<str>,
        /// The behavior spec.
        spec: Arc<dyn CallableSpec<L>>,
    },
    /// Remove the definition of `name` under `callable_type`. Removing an absent
    /// definition is an error ([`ProviderError::UndefinedName`]), never a silent no-op —
    /// the op builder can always check the visible state first.
    Remove {
        /// The invocation form the definition answers.
        callable_type: L::CallableTypeId,
        /// The (normalized) name to remove.
        name: Box<str>,
    },
}

impl<L: Lang> Clone for DefinitionOp<L> {
    fn clone(&self) -> Self {
        match self {
            DefinitionOp::Define { callable_type, name, spec } => DefinitionOp::Define {
                callable_type: *callable_type,
                name: name.clone(),
                spec: Arc::clone(spec),
            },
            DefinitionOp::Remove { callable_type, name } => {
                DefinitionOp::Remove { callable_type: *callable_type, name: name.clone() }
            }
        }
    }
}

impl<L: Lang> fmt::Debug for DefinitionOp<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DefinitionOp::Define { callable_type, name, spec } => f
                .debug_struct("Define")
                .field("callable_type", callable_type)
                .field("name", name)
                .field("spec", spec)
                .finish(),
            DefinitionOp::Remove { callable_type, name } => f
                .debug_struct("Remove")
                .field("callable_type", callable_type)
                .field("name", name)
                .finish(),
        }
    }
}

/// One scope-stack operation of a state delta
/// ([`ParsingStateDelta::scope_ops`](crate::state::ParsingStateDelta)): stack-shape ops plus definition ops routed to a named
/// provider. Ops apply in delta order, each on the result of the previous; failures are
/// collected per op (the rest still apply) and surface through the fallible
/// [`derived()`](crate::state::ParsingState::derived).
///
/// Name-targeted ops address the **innermost** provider with that name.
pub enum ScopeOp<L: Lang> {
    /// Push a provider as the new innermost entry.
    Push(Arc<dyn SpecsProvider<L>>),
    /// Remove the innermost provider named `name` from the stack. Absent name: error.
    Unload {
        /// The provider name to unload.
        name: Box<str>,
    },
    /// Replace the innermost provider named `name` with `provider` (same stack
    /// position). Absent name: error.
    Replace {
        /// The provider name to replace.
        name: Box<str>,
        /// The replacement.
        provider: Arc<dyn SpecsProvider<L>>,
    },
    /// Replace the whole stack, outermost first.
    ReplaceStack(Vec<Arc<dyn SpecsProvider<L>>>),
    /// Define `(callable_type, name)` in the provider named `scope`, through its
    /// [`with_definitions`](SpecsProvider::with_definitions) (copy-on-write: the stack
    /// entry is replaced by the returned provider). If no provider named `scope` is on
    /// the stack, a fresh [`Scope`] with that name is created **innermost**, holding
    /// the definition — scopes are created lazily on first `Define`, never eagerly per
    /// group. Targeting an immutable provider (a
    /// [`Package`]) is an error.
    Define {
        /// The name of the provider to define into.
        scope: Box<str>,
        /// The invocation form the definition answers.
        callable_type: L::CallableTypeId,
        /// The (normalized) name being defined.
        name: Box<str>,
        /// The behavior spec.
        spec: Arc<dyn CallableSpec<L>>,
    },
    /// Remove the definition of `(callable_type, name)` from the provider named
    /// `scope` (via `with_definitions`, copy-on-write). Absent provider or absent
    /// definition: error. Note that removal deletes from that provider only — a
    /// definition shadowed by *other* providers stays resolvable; "removing" a
    /// [`Package`] definition is expressed by shadowing it with an
    /// [`ErrorCallableSpec`] or unloading the package.
    Remove {
        /// The name of the provider to remove from.
        scope: Box<str>,
        /// The invocation form the definition answers.
        callable_type: L::CallableTypeId,
        /// The (normalized) name to remove.
        name: Box<str>,
    },
}

impl<L: Lang> Clone for ScopeOp<L> {
    fn clone(&self) -> Self {
        match self {
            ScopeOp::Push(provider) => ScopeOp::Push(Arc::clone(provider)),
            ScopeOp::Unload { name } => ScopeOp::Unload { name: name.clone() },
            ScopeOp::Replace { name, provider } => {
                ScopeOp::Replace { name: name.clone(), provider: Arc::clone(provider) }
            }
            ScopeOp::ReplaceStack(providers) => ScopeOp::ReplaceStack(providers.clone()),
            ScopeOp::Define { scope, callable_type, name, spec } => ScopeOp::Define {
                scope: scope.clone(),
                callable_type: *callable_type,
                name: name.clone(),
                spec: Arc::clone(spec),
            },
            ScopeOp::Remove { scope, callable_type, name } => ScopeOp::Remove {
                scope: scope.clone(),
                callable_type: *callable_type,
                name: name.clone(),
            },
        }
    }
}

impl<L: Lang> fmt::Debug for ScopeOp<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScopeOp::Push(provider) => f.debug_tuple("Push").field(provider).finish(),
            ScopeOp::Unload { name } => {
                f.debug_struct("Unload").field("name", name).finish()
            }
            ScopeOp::Replace { name, provider } => f
                .debug_struct("Replace")
                .field("name", name)
                .field("provider", provider)
                .finish(),
            ScopeOp::ReplaceStack(providers) => {
                f.debug_tuple("ReplaceStack").field(providers).finish()
            }
            ScopeOp::Define { scope, callable_type, name, spec } => f
                .debug_struct("Define")
                .field("scope", scope)
                .field("callable_type", callable_type)
                .field("name", name)
                .field("spec", spec)
                .finish(),
            ScopeOp::Remove { scope, callable_type, name } => f
                .debug_struct("Remove")
                .field("scope", scope)
                .field("callable_type", callable_type)
                .field("name", name)
                .finish(),
        }
    }
}

// --- the provider contract -------------------------------------------------------------

/// A scope-stack entry: a named source of callable definitions, with optional specials
/// participation and optional functional updates.
///
/// Entries are **all-dyn** — the multi-method contract keeps generic ops and diagnostics
/// available while admitting lazy-loading providers (large spec databases) that closed
/// data would preclude. Standard implementations: [`Package`], [`Scope`],
/// [`FallbackProvider`].
///
/// **Thread safety is part of the contract** (`Send + Sync` supertraits; see
/// [`CallableSpec`]'s note): every method takes `&self`, so a stateful implementation (a
/// memo cache, a database connection) needs interior mutability regardless — under this
/// contract that means `Mutex`/`RwLock`, not `RefCell`.
///
/// **Downcasting is part of the contract** (`Any` supertrait; [`CallableSpec`]'s
/// downcasting note applies): a consumer recovers a provider's concrete type from a
/// stored `Arc<dyn SpecsProvider<L>>` or `&dyn SpecsProvider<L>`.
///
/// **Serialization is part of the contract** ([`SerializableObject`] supertrait;
/// [`CallableSpec`]'s serialization note applies): a provider on a parsing state's
/// scope stack must be serializable through the `Arc<dyn SpecsProvider<L>>` the stack
/// holds, so the write-side capability sits on every provider type. Fully defaulted:
/// a type that does not participate in serialization adds the one-line empty impl
/// (`impl<L: Lang> SerializableObject<L> for MyProvider<L> {}`); a participating type
/// overrides [`serialize_object`](SerializableObject::serialize_object). Callable
/// only for a language that implements
/// [`SerializableLang`](crate::serialize::SerializableLang).
pub trait SpecsProvider<L: Lang>: fmt::Debug + Send + Sync + Any + SerializableObject<L> {
    /// The provider's name: how definition ops target it ([`ScopeOp::Define`] routing),
    /// and how it appears in diagnostics (the miss detail of
    /// [`ScopeStack::searched_providers`], the provider context of a
    /// [`ScopeStackError`]). Names need not be unique on a stack; name-targeted ops
    /// address the innermost match.
    fn name(&self) -> &str;

    /// Resolve `query` in this provider: `Ok(Some(spec))` on a hit, `Ok(None)` for "not
    /// defined here — continue outward" (which includes "not *visible* here": per-mode
    /// visibility is the provider's own business, checked against
    /// [`state.mode()`](ParsingState::mode); the stack is visibility-blind). `Err` means
    /// the provider itself failed — an operational error that aborts the resolution
    /// fold, never a panic.
    fn retrieve_spec(
        &self,
        query: &CallableQuery<'_, L>,
        state: &ParsingState<L>,
    ) -> Result<Option<Arc<dyn CallableSpec<L>>>, ProviderError>;

    /// Specials participation: is a callable-triggering character sequence of this
    /// provider at `content[pos..]`? Same contract as the
    /// [`Lang::scan_specials`] hook it feeds through
    /// the [`ScopeStack::scan_specials`] fold — including the [`SpecialsMatch::end`]
    /// obligations and the recovery-token protocol (the `TokenResult` shape is
    /// deliberate: a scanning provider keeps the tokenizer's recoverable-failure
    /// channel, which a [`ProviderError`] could not express). Implementations should return their **longest** match at `pos`.
    /// The default recognizes nothing.
    ///
    /// **`pos` contract:** `pos` must be a byte offset within `content`'s bounds
    /// (`pos <= content.len()`) and on a character boundary. A caller passing an
    /// invalid `pos` violates this contract; an implementation that indexes
    /// `content` with `pos` must answer with an `Err` carrying an
    /// [`ImplementationError`](crate::constructs::ImplementationError) rather
    /// than panic ([`Package`] does). The default body below and implementations
    /// that ignore `pos` answer `Ok(None)`.
    fn scan_specials(
        &self,
        state: &ParsingState<L>,
        content: &str,
        pos: usize,
    ) -> Result<Option<SpecialsMatch<L>>, SpecialsScanError> {
        let _ = (state, content, pos);
        Ok(None)
    }

    /// The characters that may start one of this provider's specials triggers — unioned
    /// over the stack at state freeze ([`ScopeStack::specials_trigger_chars`]) into the
    /// per-state filter. Must be a conservative superset of the first characters of
    /// every trigger [`scan_specials`](SpecsProvider::scan_specials) can match,
    /// *regardless of state* (there is deliberately no state parameter: the union is
    /// cached on frozen states — a mode-invisible provider's chars stay in the filter
    /// and its scan declines instead). The default: no specials.
    fn specials_trigger_chars(&self) -> TriggerChars {
        TriggerChars::default()
    }

    /// Apply definition ops **functionally**: return a fresh provider reflecting `ops`,
    /// leaving `self` untouched. This is the copy-on-write mechanism (`Arc::make_mut`
    /// does not exist for `dyn` types): the stack replaces its entry with the returned
    /// `Arc`, and outer states keep holding the old one — scoped reversion stays
    /// structural. Atomic per call: on `Err`, no partial application may be observable.
    ///
    /// The default refuses ([`ProviderError::NotMutable`]) — correct for immutable
    /// providers ([`Package`], [`FallbackProvider`]); [`Scope`] implements it.
    fn with_definitions(
        &self,
        ops: &[DefinitionOp<L>],
    ) -> Result<Arc<dyn SpecsProvider<L>>, ProviderError> {
        let _ = ops;
        Err(ProviderError::NotMutable)
    }

    /// Enumerate this provider's definitions of the invocation form `callable_type`
    /// that are visible under `mode`.
    /// Visibility is the provider's own business, exactly as in
    /// [`retrieve_spec`](SpecsProvider::retrieve_spec) — and since visibility is
    /// mode-determined at every grain, the mode alone is passed, not a full state.
    ///
    /// `None` = this provider **cannot enumerate** (the default — correct for
    /// [`FallbackProvider`], which answers *any* name); `Some` of an empty iterator =
    /// enumerable and nothing visible. Specials definitions enumerate under their
    /// registered callable type with the **trigger spelling as the name**. Yield order
    /// within a provider is unspecified.
    fn iter_symbols(
        &self,
        callable_type: L::CallableTypeId,
        mode: L::ModeId,
    ) -> Option<Box<dyn Iterator<Item = SymbolEntry<'_, L>> + '_>> {
        let _ = (callable_type, mode);
        None
    }
}

mod sealed {
    use super::{Lang, Package, SpecsProvider};
    use alloc::sync::Arc;

    pub trait SealedProvider<L: Lang> {}
    impl<L: Lang> SealedProvider<L> for Package<L> {}
    impl<L: Lang, P: SpecsProvider<L> + 'static> SealedProvider<L> for Arc<P> {}
    impl<L: Lang> SealedProvider<L> for Arc<dyn SpecsProvider<L>> {}
}

/// Sealed conversion into a shared [`SpecsProvider`] — the item contract of
/// [`ParsingState::lang_initial_with_packages`](crate::state::ParsingState::lang_initial_with_packages),
/// following the crate's one Arc-removal conversion idiom (the spec-side sibling is
/// [`IntoCallableSpec`](crate::spec::IntoCallableSpec)): a [`Package`] passes **by
/// value** (`lang_initial_with_packages([minilatex_package(), my_pkg])`, no `Arc::new`
/// noise), while an already-shared **`Arc<P>`** or **`Arc<dyn SpecsProvider<L>>`**
/// passes through as-is — no double-wrap.
///
/// Sealed: the three impls above are the whole vocabulary; downstream code implements
/// [`SpecsProvider`], never this trait.
pub trait IntoSpecsProvider<L: Lang>: sealed::SealedProvider<L> {
    /// Convert into the shared provider handle scope stacks store.
    fn into_specs_provider(self) -> Arc<dyn SpecsProvider<L>>;
}

impl<L: Lang> IntoSpecsProvider<L> for Package<L> {
    fn into_specs_provider(self) -> Arc<dyn SpecsProvider<L>> {
        Arc::new(self)
    }
}

impl<L: Lang, P: SpecsProvider<L> + 'static> IntoSpecsProvider<L> for Arc<P> {
    fn into_specs_provider(self) -> Arc<dyn SpecsProvider<L>> {
        self
    }
}

impl<L: Lang> IntoSpecsProvider<L> for Arc<dyn SpecsProvider<L>> {
    fn into_specs_provider(self) -> Arc<dyn SpecsProvider<L>> {
        self
    }
}

/// One enumerated definition (see [`SpecsProvider::iter_symbols`]): a borrowed,
/// self-describing row — callers clone the `Arc` if they keep it.
pub struct SymbolEntry<'a, L: Lang> {
    /// The invocation form the definition is registered under.
    pub callable_type: L::CallableTypeId,
    /// The defined name — for a specials definition, the trigger spelling (`"--"`).
    pub name: &'a str,
    /// The definition's behavior spec.
    pub spec: &'a Arc<dyn CallableSpec<L>>,
}

// Manual impls: `SymbolEntry` is Copy regardless of `L` (an id and two borrows).

impl<L: Lang> Clone for SymbolEntry<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang> Copy for SymbolEntry<'_, L> {}

impl<L: Lang> fmt::Debug for SymbolEntry<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SymbolEntry")
            .field("callable_type", &self.callable_type)
            .field("name", &self.name)
            .field("spec", &self.spec)
            .finish()
    }
}

// --- the parse-initialization escape-shadowing check -------------------------------------

/// Condition (warning): every definition a provider advertises under one callable
/// type begins with an escape character — the registration trap where names were
/// inserted *with* their escape character (`"\greet"` instead of `"greet"`,
/// [`Package::insert`]'s normalized-name contract), leaving the whole table
/// unreachable: command tokens carry their name without the escape character, so
/// none of these definitions can ever resolve.
///
/// Emitted by [`check_provider_commands_shadowed_by_escape`] at parse
/// initialization (the latexlike preset wires it for every parse).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.specs.provider-commands-shadowed-by-escape")]
pub struct ProviderCommandsShadowedByEscape {
    /// The provider whose definitions are all escape-shadowed.
    pub provider: String,
    /// The affected callable type (its `Debug` rendering — the wire payload cannot
    /// carry a language's type id directly).
    pub callable_type: String,
    /// How many definitions the provider advertises under that type (all of them
    /// escape-shadowed).
    pub count: usize,
    /// One offending name, exactly as registered (e.g. `"\\greet"`).
    pub example: String,
    /// The escape character(s) the offending names begin with.
    pub escape_chars: String,
}

// Hand-written wording: the count needs number agreement, which the derive's message
// format string cannot express.
impl fmt::Display for ProviderCommandsShadowedByEscape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "every ‘{}’ definition of provider ‘{}’ begins with an escape character \
             (‘{}’ — {} definition{}, e.g. ‘{}’): command names are registered \
             without the escape character, so none of these definitions can resolve",
            self.callable_type,
            self.provider,
            self.escape_chars,
            self.count,
            if self.count == 1 { "" } else { "s" },
            self.example,
        )
    }
}

/// Check the state's seeded providers for all-escape-shadowed definition tables
/// and record one [`ProviderCommandsShadowedByEscape`] **warning** per affected
/// (provider, callable type) — the parse-initialization half of the registration
/// trap's defenses (the resolution-miss half is
/// [`resolve_command_in_scopes`](crate::engine::resolve_command_in_scopes)'s
/// did-you-mean detail, which an in-stack fallback provider can mute; this check
/// fires regardless of fallbacks).
///
/// For each provider on `state`'s scope stack and each callable type in the
/// language's vocabulary, the provider's advertised names
/// ([`SpecsProvider::iter_symbols`], unioned over every mode) are tested against
/// the state's command escape characters
/// ([`TokenRules::command_rules`](crate::token::TokenRules::command_rules)): when **all**
/// (≥ 1) of them begin with an escape character, a warning is recorded at
/// `source`'s start. Nothing is checked when commands are disabled, and providers
/// that cannot enumerate (a [`FallbackProvider`]) are skipped — the check is a
/// best-effort diagnostics nicety, not semantics.
///
/// **Bound where used** ("provide, don't require"): enumerating *vocabularies*
/// needs [`ClosedVocabulary`] on the callable-type and mode ids, so the bound sits
/// on this function alone — never on [`Lang`](crate::state::Lang) or the latexlike
/// role traits. The latexlike preset calls it unconditionally at parse
/// initialization (its vocabularies are enumerable); a family member or framework
/// with enumerable vocabularies wires it the same way
/// ([`LatexlikeLang::check_parse_start`](crate::latexlike::LatexlikeLang::check_parse_start),
/// or directly at its own parse entry); for non-enumerable vocabularies the check
/// is gracefully absent.
pub fn check_provider_commands_shadowed_by_escape<L: Lang>(
    state: &ParsingState<L>,
    source: &Arc<Source<L::SourceOrigin>>,
    diagnostics: &mut Diagnostics<L::SourceOrigin>,
) where
    L::CallableTypeId: ClosedVocabulary,
    L::ModeId: ClosedVocabulary,
{
    let rules = state.rules();
    if !rules.commands_enabled() || rules.command_rules().is_empty() {
        return;
    }
    let escape_chars: Vec<char> =
        rules.command_rules().iter().map(|rule| rule.escape_char).collect();

    for provider in state.scopes().providers().iter().rev() {
        for &callable_type in L::CallableTypeId::ALL {
            // The provider's advertised names under this type, unioned over every
            // mode (deduplicated; sorted so the reported example is deterministic).
            let mut names: Vec<Box<str>> = Vec::new();
            for &mode in L::ModeId::ALL {
                let Some(symbols) = provider.iter_symbols(callable_type, mode) else {
                    continue; // cannot enumerate: gracefully absent
                };
                for entry in symbols {
                    if !names.iter().any(|name| &**name == entry.name) {
                        names.push(entry.name.into());
                    }
                }
            }
            if names.is_empty() {
                continue;
            }
            names.sort_unstable();
            let shadowed = |name: &str| {
                name.chars().next().is_some_and(|first| escape_chars.contains(&first))
            };
            if !names.iter().all(|name| shadowed(name)) {
                continue;
            }
            let mut offending_escapes: Vec<char> = names
                .iter()
                .filter_map(|name| name.chars().next())
                .collect();
            offending_escapes.sort_unstable();
            offending_escapes.dedup();
            diagnostics.push(Diagnostic::warning(
                ProviderCommandsShadowedByEscape {
                    provider: provider.name().into(),
                    callable_type: alloc::format!("{callable_type:?}"),
                    count: names.len(),
                    example: names[0].clone().into(),
                    escape_chars: offending_escapes.into_iter().collect(),
                },
                SourceSpan::new(source, Span::empty(0)),
            ));
        }
    }
}

// --- Package ----------------------------------------------------------------------------

/// One data-driven specials entry of a [`Package`]: a trigger string resolving to a
/// callable — pylatexenc's specials-as-category-data, kept as plain data so packages are
/// loadable wholesale (ligature specials like `---`, `~`, `&`).
struct PackageSpecials<L: Lang> {
    trigger: Box<str>,
    callable_type: L::CallableTypeId,
    spec: Arc<dyn CallableSpec<L>>,
    /// Per-entry mode visibility (`None` = every mode the package is visible in).
    visible_modes: Option<Vec<L::ModeId>>,
}

impl<L: Lang> Clone for PackageSpecials<L> {
    fn clone(&self) -> Self {
        PackageSpecials {
            trigger: self.trigger.clone(),
            callable_type: self.callable_type,
            spec: Arc::clone(&self.spec),
            visible_modes: self.visible_modes.clone(),
        }
    }
}

/// One callable definition of a [`Package`]: the spec plus its per-entry mode
/// visibility. `None` visible modes = visible in every mode the package itself is (the
/// package-level [`set_visible_modes`](Package::set_visible_modes) is the coarse gate,
/// this is the fine one).
struct PackageEntry<L: Lang> {
    spec: Arc<dyn CallableSpec<L>>,
    visible_modes: Option<Vec<L::ModeId>>,
}

impl<L: Lang> Clone for PackageEntry<L> {
    fn clone(&self) -> Self {
        PackageEntry {
            spec: Arc::clone(&self.spec),
            visible_modes: self.visible_modes.clone(),
        }
    }
}

/// Whether a per-entry mode restriction admits `mode` (`None` = every mode).
fn modes_admit<M: Eq>(visible_modes: &Option<Vec<M>>, mode: &M) -> bool {
    match visible_modes {
        None => true,
        Some(modes) => modes.contains(mode),
    }
}

/// An immutable, wholesale-loaded collection of definitions — the standard
/// [`SpecsProvider`] for preset/library data.
///
/// Built once (insertions while owned), then shared behind an `Arc` on scope stacks;
/// it never mutates afterwards — its [`with_definitions`](SpecsProvider::with_definitions)
/// is the default refusal. Keys are `(Lang::CallableTypeId, normalized name)`,
/// **many-to-one**: several names may map to one shared spec (flyweight — `\emph` and
/// `\textit` can share an `Arc`). Inserting an existing key replaces the previous spec.
///
/// **Mode visibility** is checked inside the package's own
/// [`retrieve_spec`](SpecsProvider::retrieve_spec)/[`scan_specials`](SpecsProvider::scan_specials)
/// (the stack is visibility-blind), at two grains: a **package-level** gate
/// ([`set_visible_modes`](Package::set_visible_modes)) and a **per-entry** gate
/// ([`insert_in_modes`](Package::insert_in_modes)/[`insert_specials_in_modes`](Package::insert_specials_in_modes)).
/// Both must admit [`ParsingState::mode`] for a definition to resolve; outside its modes
/// a package or an entry answers `Ok(None)` — invisible, not an error. So one loadable
/// package can hold text-only typography specials and math-only scripts side by side.
///
/// **Specials as data**: a package may carry trigger-string definitions
/// ([`insert_specials`](Package::insert_specials)); its scan returns the longest
/// matching trigger.
///
/// **Serialization.** A package is serialized by *identity*: its entry carries the
/// package's name only, and the reading side looks the name up among the providers
/// it already holds ([`KnownProviders`](crate::serialize::KnownProviders)) — a package
/// is part of the reading program's own configuration, not something to describe in
/// full. Its *definitions* are
/// serialized by identity too, when the package was built **shared**
/// ([`new_shared`](Package::new_shared)): the specs it hands out
/// [`SpecProvenance`] stamps for ([`provenance_for`](Package::provenance_for),
/// [`provenance_for_specials`](Package::provenance_for_specials)) refer back to it.
/// A package built with [`new`](Package::new) and later wrapped in an `Arc` cannot
/// stamp its specs (it has no reference to its own `Arc`), so its specs — unless
/// their type has a self-contained serialized form — cannot be serialized; the
/// package itself still can.
pub struct Package<L: Lang> {
    name: Box<str>,
    specs: HashMap<L::CallableTypeId, HashMap<Box<str>, PackageEntry<L>>>,
    /// Sorted by trigger byte length, longest first (the package-internal longest-match
    /// rule); one entry per trigger string.
    specials: Vec<PackageSpecials<L>>,
    /// `None` = visible in every mode.
    visible_modes: Option<Vec<L::ModeId>>,
    /// The package's own `Arc`, weakly, when built shared ([`Package::new_shared`]):
    /// what its provenance stamps refer to. `None` for a package built with
    /// [`Package::new`].
    self_weak: Option<Weak<dyn SpecsProvider<L>>>,
}

impl<L: Lang> Package<L> {
    /// An empty package. The name identifies it on stacks, in ops, and in diagnostics.
    ///
    /// A package built this way cannot hand out provenance stamps for its definitions
    /// (see [`new_shared`](Package::new_shared) and the type's documentation).
    pub fn new(name: impl Into<Box<str>>) -> Package<L> {
        Package {
            name: name.into(),
            specs: HashMap::new(),
            specials: Vec::new(),
            visible_modes: None,
            self_weak: None,
        }
    }

    /// An empty package that `build` fills, returned shared: the package is created
    /// inside its own `Arc`, so that while `build` runs it can hand out
    /// [`SpecProvenance`] stamps for its definitions
    /// ([`provenance_for`](Package::provenance_for),
    /// [`provenance_for_specials`](Package::provenance_for_specials)) — the stamps
    /// refer weakly to the `Arc` being built. Inside `build`, insert the definitions
    /// as usual, stamping the specs first (the crate's spec types take the stamp with
    /// `with_provenance`; the latexlike helpers `define_macro`/`define_environment`
    /// stamp automatically). Only the specs a package stamps can be serialized by
    /// identity — see the type's documentation.
    ///
    /// ```
    /// use techy::core::specs::{Package, StdCallableSpec};
    /// use techy::core::TrivialLang;
    ///
    /// #[derive(Debug, Clone, Copy)]
    /// struct MyLang;
    /// impl TrivialLang for MyLang {}
    ///
    /// const MACRO: u32 = 0;
    /// let package = Package::<MyLang>::new_shared("mydefs", |package| {
    ///     let stamp = package.provenance_for(MACRO, "emph").expect("a shared package stamps");
    ///     package.insert(MACRO, "emph", StdCallableSpec::default().with_provenance(stamp));
    /// });
    /// assert_eq!(package.name(), "mydefs");
    /// assert!(package.get(MACRO, "emph").is_some());
    /// ```
    pub fn new_shared(
        name: impl Into<Box<str>>,
        build: impl FnOnce(&mut Package<L>),
    ) -> Arc<Package<L>> {
        Arc::new_cyclic(|weak: &Weak<Package<L>>| {
            let mut package = Package::new(name);
            package.self_weak = Some(Weak::clone(weak) as Weak<dyn SpecsProvider<L>>);
            build(&mut package);
            package
        })
    }

    /// This package's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether this package was built shared ([`new_shared`](Package::new_shared))
    /// and so hands out provenance stamps.
    pub fn is_shared(&self) -> bool {
        self.self_weak.is_some()
    }

    /// The provenance stamp for the definition of `name` under `callable_type` in this
    /// package — for a spec about to be inserted under that key (or already there):
    /// the stamp names this package (weakly), the invocation form, and
    /// [`DefinitionKey::Name`]. `None` unless the package was built shared
    /// ([`new_shared`](Package::new_shared)); nothing checks that the key is or
    /// becomes defined — the caller inserts the stamped spec under it.
    pub fn provenance_for(
        &self,
        callable_type: L::CallableTypeId,
        name: impl Into<Box<str>>,
    ) -> Option<SpecProvenance<L>> {
        let provider = self.self_weak.as_ref()?;
        Some(SpecProvenance::new(Weak::clone(provider), callable_type, DefinitionKey::Name(name.into())))
    }

    /// The provenance stamp for the specials definition of `trigger` under
    /// `callable_type` — the [`provenance_for`](Package::provenance_for) sibling for
    /// [`insert_specials`](Package::insert_specials) entries
    /// ([`DefinitionKey::Trigger`]). `None` unless the package was built shared.
    pub fn provenance_for_specials(
        &self,
        callable_type: L::CallableTypeId,
        trigger: impl Into<Box<str>>,
    ) -> Option<SpecProvenance<L>> {
        let provider = self.self_weak.as_ref()?;
        Some(SpecProvenance::new(
            Weak::clone(provider),
            callable_type,
            DefinitionKey::Trigger(trigger.into()),
        ))
    }

    /// Define `name` (normalized spelling) under the invocation form `callable_type`,
    /// visible in every mode the package is. Returns the spec previously defined under
    /// that key, if any.
    ///
    /// **The name is the *normalized* spelling — never include the escape
    /// character.** Register `"emph"`, not `"\\emph"`: command tokens carry their
    /// name *without* the escape character, so an escape-prefixed registration can
    /// never match — the definition is silently unreachable. This is deliberately
    /// *not* validated here: escape characters are a [`TokenRules`](crate::token::TokenRules)
    /// fact this layer cannot know, they can change mid-parse, and a leading
    /// escape-character-like char can be fully intended (`@greet` registered before
    /// `@` *becomes* an escape character). The trap is caught where it bites
    /// instead: the resolution-miss detail suggests escape-prefixed near-misses
    /// ([`resolve_command_in_scopes`](crate::engine::resolve_command_in_scopes)),
    /// and a parse-initialization check warns when *all* of a provider's
    /// definitions are escape-shadowed
    /// ([`check_provider_commands_shadowed_by_escape`]).
    ///
    /// **No spec-type/callable-type cross-check either — deliberately.** A
    /// "mismatched" registration (say, a plain macro-shaped spec under an
    /// environment form) is documented-legitimate: the *composition* that parses
    /// the invocation form owns the parse, and the spec contributes argument
    /// structure — e.g. the latexlike environment composition parses any
    /// `CallableSpec`'s declared arguments after `\begin{name}` and gives the body
    /// the default handling. The preset one-liners (`define_macro`/
    /// `define_environment` on latexlike packages) make the correct pairing
    /// structural in normal use.
    ///
    /// The spec passes through the sealed [`IntoCallableSpec`] conversion: by value
    /// (`insert(CallableType::Macro, "emph", MacroSpec::new(…))` — no `Arc::new`), or
    /// pre-shared as an `Arc` for flyweight sharing across names.
    pub fn insert<M>(
        &mut self,
        callable_type: L::CallableTypeId,
        name: impl Into<Box<str>>,
        spec: impl IntoCallableSpec<L, M>,
    ) -> Option<Arc<dyn CallableSpec<L>>> {
        self.insert_in_modes(callable_type, name, spec, None)
    }

    /// Like [`insert`](Package::insert), but with per-entry mode visibility: the
    /// definition resolves only when [`ParsingState::mode`] is one of `visible_modes`
    /// (`None` = every mode the package is visible in — the [`insert`](Package::insert)
    /// default). This is the fine gate under the package-level
    /// [`set_visible_modes`](Package::set_visible_modes): both must admit the mode. A
    /// text-only accent and a math-only script can live in one loadable package.
    pub fn insert_in_modes<M>(
        &mut self,
        callable_type: L::CallableTypeId,
        name: impl Into<Box<str>>,
        spec: impl IntoCallableSpec<L, M>,
        visible_modes: Option<Vec<L::ModeId>>,
    ) -> Option<Arc<dyn CallableSpec<L>>> {
        self.specs
            .entry(callable_type)
            .or_default()
            .insert(
                name.into(),
                PackageEntry { spec: spec.into_callable_spec(), visible_modes },
            )
            .map(|previous| previous.spec)
    }

    /// Define the specials `trigger` (e.g. `"---"`) as resolving to `spec` under the
    /// invocation form `callable_type`, visible in every mode the package is. One entry
    /// per trigger string: inserting an existing trigger replaces it (and returns the
    /// previous spec). The spec passes through the sealed [`IntoCallableSpec`]
    /// conversion, exactly as in [`insert`](Package::insert) — whose parameter order
    /// (callable type first, then the key) this method mirrors.
    pub fn insert_specials<M>(
        &mut self,
        callable_type: L::CallableTypeId,
        trigger: impl Into<Box<str>>,
        spec: impl IntoCallableSpec<L, M>,
    ) -> Option<Arc<dyn CallableSpec<L>>> {
        self.insert_specials_in_modes(callable_type, trigger, spec, None)
    }

    /// Like [`insert_specials`](Package::insert_specials), but with per-entry mode
    /// visibility: the trigger is recognized only when [`ParsingState::mode`] is one of
    /// `visible_modes` (`None` = every mode the package is visible in). Text-mode
    /// typography ligatures (`` `` ``, `---`) and math-mode scripts can share one
    /// package, each firing only in its own mode.
    pub fn insert_specials_in_modes<M>(
        &mut self,
        callable_type: L::CallableTypeId,
        trigger: impl Into<Box<str>>,
        spec: impl IntoCallableSpec<L, M>,
        visible_modes: Option<Vec<L::ModeId>>,
    ) -> Option<Arc<dyn CallableSpec<L>>> {
        let spec = spec.into_callable_spec();
        let trigger = trigger.into();
        if let Some(existing) =
            self.specials.iter_mut().find(|entry| entry.trigger == trigger)
        {
            existing.callable_type = callable_type;
            existing.visible_modes = visible_modes;
            return Some(core::mem::replace(&mut existing.spec, spec));
        }
        self.specials.push(PackageSpecials { trigger, callable_type, spec, visible_modes });
        // Longest first; the stable sort keeps insertion order among equal lengths
        // (irrelevant for matching — equal-length triggers differ in content).
        self.specials.sort_by_key(|entry| core::cmp::Reverse(entry.trigger.len()));
        None
    }

    /// Restrict the *whole package* to the given parsing modes (`None` = visible
    /// everywhere, the default) — the coarse gate. Visibility is checked by the package
    /// itself, against [`ParsingState::mode`]: outside its modes the package answers
    /// "not here" for resolution and specials alike. For per-definition control under
    /// this gate, see [`insert_in_modes`](Package::insert_in_modes) and
    /// [`insert_specials_in_modes`](Package::insert_specials_in_modes).
    pub fn set_visible_modes(&mut self, modes: Option<Vec<L::ModeId>>) {
        self.visible_modes = modes;
    }

    /// The spec defined under `(callable_type, name)`, if any (visibility-blind: this
    /// is the raw data accessor; mode checks apply only on the provider paths).
    ///
    /// **Specials are not reachable here.** They are keyed by **trigger**, not by
    /// name, and live in their own store — `get` answers `None` for a specials
    /// definition even immediately after
    /// [`insert_specials`](Package::insert_specials) (whose `callable_type`
    /// parameter is what the *match* reports, not a lookup key here). To read
    /// them back, use [`get_specials`](Package::get_specials) (by trigger),
    /// [`iter_symbols`](SpecsProvider::iter_symbols) (which lists
    /// specials with the trigger as the name) or
    /// [`scan_specials`](SpecsProvider::scan_specials) (the matching path);
    /// [`len`](Package::len) counts them.
    pub fn get(
        &self,
        callable_type: L::CallableTypeId,
        name: &str,
    ) -> Option<&Arc<dyn CallableSpec<L>>> {
        self.specs.get(&callable_type)?.get(name).map(|entry| &entry.spec)
    }

    /// The spec of the specials definition registered for `trigger` under
    /// `callable_type`, if any (visibility-blind, like [`get`](Package::get)): the
    /// raw accessor of the specials store, keyed by the trigger string exactly as
    /// registered ([`insert_specials`](Package::insert_specials)). The callable type
    /// must be the one the entry was registered with — a trigger registered under
    /// another type answers `None`.
    pub fn get_specials(
        &self,
        callable_type: L::CallableTypeId,
        trigger: &str,
    ) -> Option<&Arc<dyn CallableSpec<L>>> {
        self.specials
            .iter()
            .find(|entry| entry.callable_type == callable_type && &*entry.trigger == trigger)
            .map(|entry| &entry.spec)
    }

    /// Number of definitions across all callable types (specials entries included).
    pub fn len(&self) -> usize {
        self.specs.values().map(HashMap::len).sum::<usize>() + self.specials.len()
    }

    /// Whether the package holds no definitions at all.
    pub fn is_empty(&self) -> bool {
        self.specs.values().all(HashMap::is_empty) && self.specials.is_empty()
    }

    fn visible_in(&self, mode: L::ModeId) -> bool {
        modes_admit(&self.visible_modes, &mode)
    }
}

// The `SerializableObject` (identity by name) and `DeserializableObject` impls live
// in `crate::serialize::drivers::specs`, with the other core spec/provider types'.

impl<L: Lang> SpecsProvider<L> for Package<L> {
    fn name(&self) -> &str {
        &self.name
    }

    fn retrieve_spec(
        &self,
        query: &CallableQuery<'_, L>,
        state: &ParsingState<L>,
    ) -> Result<Option<Arc<dyn CallableSpec<L>>>, ProviderError> {
        if !self.visible_in(state.mode()) {
            return Ok(None);
        }
        let Some(entry) =
            self.specs.get(&query.callable_type).and_then(|defs| defs.get(query.name))
        else {
            return Ok(None);
        };
        // Per-entry mode gate under the package-level one (both must admit the mode).
        if !modes_admit(&entry.visible_modes, &state.mode()) {
            return Ok(None);
        }
        Ok(Some(Arc::clone(&entry.spec)))
    }

    /// See the trait method's `pos` contract: an invalid `pos` (out of
    /// `content`'s bounds, or not a character boundary) answers an `Err`
    /// carrying an [`ImplementationError`] whose message names which of the two
    /// cases applies — checked before the visibility gate, so a caller bug is
    /// reported regardless of the package's mode visibility.
    fn scan_specials(
        &self,
        state: &ParsingState<L>,
        content: &str,
        pos: usize,
    ) -> Result<Option<SpecialsMatch<L>>, SpecialsScanError> {
        // Caller contract violation — same error spelling as the token
        // reader's own `Lang::scan_specials` match-end validation: a
        // `Custom`-wrapped `ImplementationError`, no recovery (an
        // implementation bug aborts even under tolerant recovery). The error
        // span is clamped to the content so it stays liftable to a source
        // span.
        let Some(rest) = content.get(pos..) else {
            let case = if pos > content.len() {
                "out of the content's bounds"
            } else {
                "not a character boundary"
            };
            return Err(SpecialsScanError {
                kind: TokenErrorKind::Custom(Box::new(ImplementationError::new(
                    alloc::format!(
                        "Package::scan_specials called with an invalid position {pos} \
                         ({case}; content length {})",
                        content.len()
                    ),
                ))),
                span: Span::empty(pos.min(content.len())),
            });
        };
        if !self.visible_in(state.mode()) {
            return Ok(None);
        }
        // Entries are sorted longest-first: the first hit is the package's longest.
        for entry in &self.specials {
            // A mode-invisible entry is not there: skip it (a shorter visible trigger
            // may still match). The trigger-char union stays mode-blind, so the
            // tokenizer still probes here and this scan declines.
            if !modes_admit(&entry.visible_modes, &state.mode()) {
                continue;
            }
            if rest.starts_with(&*entry.trigger) {
                let end = pos + entry.trigger.len();
                return Ok(Some(SpecialsMatch {
                    end,
                    callable_type: entry.callable_type,
                    spec: Arc::clone(&entry.spec),
                }));
            }
        }
        Ok(None)
    }

    fn specials_trigger_chars(&self) -> TriggerChars {
        let mut chars = String::new();
        for entry in &self.specials {
            if let Some(first) = entry.trigger.chars().next() {
                if !chars.contains(first) {
                    chars.push(first);
                }
            }
        }
        TriggerChars::Only(chars)
    }

    fn iter_symbols(
        &self,
        callable_type: L::CallableTypeId,
        mode: L::ModeId,
    ) -> Option<Box<dyn Iterator<Item = SymbolEntry<'_, L>> + '_>> {
        // Outside the package-level gate: enumerable, nothing visible.
        if !self.visible_in(mode) {
            return Some(Box::new(core::iter::empty()));
        }
        // Both storage tables contribute by their entries' *recorded* type — the
        // package never asks which type "means" specials.
        let named = self
            .specs
            .get(&callable_type)
            .into_iter()
            .flat_map(HashMap::iter)
            .filter(move |(_, entry)| modes_admit(&entry.visible_modes, &mode))
            .map(move |(name, entry)| SymbolEntry {
                callable_type,
                name,
                spec: &entry.spec,
            });
        let specials = self
            .specials
            .iter()
            .filter(move |entry| {
                entry.callable_type == callable_type && modes_admit(&entry.visible_modes, &mode)
            })
            .map(move |entry| SymbolEntry {
                callable_type,
                name: &entry.trigger,
                spec: &entry.spec,
            });
        Some(Box::new(named.chain(specials)))
    }
}

/// A clone is a new, unshared package: it copies the name, the definitions (sharing
/// their spec `Arc`s), and the visibility, but not the shared identity — the clone
/// hands out no provenance stamps of its own, and the stamps its specs may carry keep
/// naming the package they were cloned from.
///
/// Consequence for serialization: a parse driven by a clone (the clone in the scope
/// stack, its specs stamped by the original) serializes the clone and the original
/// as two provider entries of one name — the states refer to the clone, the specs to
/// the original — and a reading side that resolves the name through a
/// [`ProviderRecipe`](crate::serialize::ProviderRecipe) alone rebuilds the two entries
/// as two packages, so the spec a tree node holds is not the instance held by the
/// package in the rebuilt state's scope stack (a reading side that holds the very
/// `Arc`s in its [`KnownProviders`](crate::serialize::KnownProviders) resolves both
/// entries to that one instance).
impl<L: Lang> Clone for Package<L> {
    fn clone(&self) -> Self {
        Package {
            name: self.name.clone(),
            specs: self.specs.clone(),
            specials: self.specials.clone(),
            visible_modes: self.visible_modes.clone(),
            self_weak: None,
        }
    }
}

impl<L: Lang> fmt::Debug for Package<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Package")
            .field("name", &self.name)
            .field("definitions", &self.len())
            .field("visible_modes", &self.visible_modes)
            .field("shared", &self.is_shared())
            .finish()
    }
}

// --- Scope -------------------------------------------------------------------------------

/// The definition target: the standard mutable-by-replacement [`SpecsProvider`] that
/// [`ScopeOp::Define`]/[`ScopeOp::Remove`] address.
///
/// "Mutable" means **copy-on-write**: [`with_definitions`](SpecsProvider::with_definitions)
/// returns a fresh `Scope` and the stack swaps its entry, while outer states keep the
/// old `Arc` — group-local definition semantics fall out of structural reversion, even
/// when the targeted scope sits below other providers. Storage is a `BTreeMap` (`no_std`, deterministic iteration); keys are
/// `(Lang::CallableTypeId, normalized name)`, many-to-one to shared specs.
///
/// **Serialization.** A scope holds definitions made during a parse, so it is
/// serialized in full — its name and every definition, each spec as an entry of its
/// own (by identity or in a self-contained form, whatever the spec's type writes) —
/// and rebuilt through [`new`](Scope::new) and [`insert`](Scope::insert).
pub struct Scope<L: Lang> {
    name: Box<str>,
    #[allow(clippy::type_complexity)]
    specs: BTreeMap<L::CallableTypeId, BTreeMap<Box<str>, Arc<dyn CallableSpec<L>>>>,
}

impl<L: Lang> Scope<L> {
    /// An empty scope. The name is how definition ops target it.
    pub fn new(name: impl Into<Box<str>>) -> Scope<L> {
        Scope { name: name.into(), specs: BTreeMap::new() }
    }

    /// This scope's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Define `name` under `callable_type` directly (builder-style, while the scope is
    /// still owned — seeding initial states, tests). Returns the previously defined
    /// spec, if any. Mid-parse definitions go through [`ScopeOp::Define`] instead.
    pub fn insert(
        &mut self,
        callable_type: L::CallableTypeId,
        name: impl Into<Box<str>>,
        spec: Arc<dyn CallableSpec<L>>,
    ) -> Option<Arc<dyn CallableSpec<L>>> {
        self.specs.entry(callable_type).or_default().insert(name.into(), spec)
    }

    /// Remove the definition of `name` under `callable_type` directly (builder-style).
    /// Returns the removed spec, or `None` if it was not defined.
    pub fn remove(
        &mut self,
        callable_type: L::CallableTypeId,
        name: &str,
    ) -> Option<Arc<dyn CallableSpec<L>>> {
        self.specs.get_mut(&callable_type)?.remove(name)
    }

    /// The spec defined under `(callable_type, name)`, if any.
    pub fn get(
        &self,
        callable_type: L::CallableTypeId,
        name: &str,
    ) -> Option<&Arc<dyn CallableSpec<L>>> {
        self.specs.get(&callable_type)?.get(name)
    }

    /// Number of definitions across all callable types.
    pub fn len(&self) -> usize {
        self.specs.values().map(BTreeMap::len).sum()
    }

    /// Whether the scope holds no definitions.
    pub fn is_empty(&self) -> bool {
        self.specs.values().all(BTreeMap::is_empty)
    }

    /// Every definition — `(callable type, name, spec)` — ordered by callable type,
    /// then by name (the storage order; deterministic).
    pub fn definitions(
        &self,
    ) -> impl Iterator<Item = (L::CallableTypeId, &str, &Arc<dyn CallableSpec<L>>)> + '_ {
        self.specs.iter().flat_map(|(callable_type, definitions)| {
            definitions.iter().map(move |(name, spec)| (*callable_type, &**name, spec))
        })
    }
}

// The `SerializableObject` (the definitions in full) and `DeserializableObject`
// impls live in `crate::serialize::drivers::specs`.

impl<L: Lang> SpecsProvider<L> for Scope<L> {
    fn name(&self) -> &str {
        &self.name
    }

    fn retrieve_spec(
        &self,
        query: &CallableQuery<'_, L>,
        _state: &ParsingState<L>,
    ) -> Result<Option<Arc<dyn CallableSpec<L>>>, ProviderError> {
        Ok(self.get(query.callable_type, query.name).cloned())
    }

    /// Copy-on-write: clone the maps, apply `ops` in order, return a fresh `Scope`.
    /// Atomic — the first failing op discards the copy and nothing is observable.
    fn with_definitions(
        &self,
        ops: &[DefinitionOp<L>],
    ) -> Result<Arc<dyn SpecsProvider<L>>, ProviderError> {
        let mut fresh = self.clone();
        for op in ops {
            match op {
                DefinitionOp::Define { callable_type, name, spec } => {
                    fresh.insert(*callable_type, name.clone(), Arc::clone(spec));
                }
                DefinitionOp::Remove { callable_type, name } => {
                    if fresh.remove(*callable_type, name).is_none() {
                        return Err(ProviderError::UndefinedName { name: name.clone() });
                    }
                }
            }
        }
        Ok(Arc::new(fresh))
    }

    fn iter_symbols(
        &self,
        callable_type: L::CallableTypeId,
        mode: L::ModeId,
    ) -> Option<Box<dyn Iterator<Item = SymbolEntry<'_, L>> + '_>> {
        // Scope definitions carry no visibility — every mode sees them all.
        let _ = mode;
        let entries = self
            .specs
            .get(&callable_type)
            .into_iter()
            .flat_map(BTreeMap::iter)
            .map(move |(name, spec)| SymbolEntry { callable_type, name, spec });
        Some(Box::new(entries))
    }
}

impl<L: Lang> Clone for Scope<L> {
    fn clone(&self) -> Self {
        Scope { name: self.name.clone(), specs: self.specs.clone() }
    }
}

impl<L: Lang> fmt::Debug for Scope<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scope")
            .field("name", &self.name)
            .field("definitions", &self.len())
            .finish()
    }
}

// --- FallbackProvider --------------------------------------------------------------------

/// The unknown-callable policy as an ordinary provider: answers **any** name of its
/// registered callable types with that type's shared fallback singleton (fallbacks live *in* the stack, at the bottom, so
/// suppression by shadowing is a theorem of search order, and the stack needs no
/// separate fallback map).
///
/// Fallback specs are shared singletons (possible because specs are de-keyed), so
/// "unknown `\foo`" costs no per-instance allocation, and a callable node's spec can be
/// guaranteed never-`None` for every callable type registered here.
///
/// **Serialization.** Serialized in full — its name and every fallback, each spec as
/// an entry of its own — and rebuilt through [`new`](FallbackProvider::new) and
/// [`set`](FallbackProvider::set).
pub struct FallbackProvider<L: Lang> {
    name: Box<str>,
    fallbacks: BTreeMap<L::CallableTypeId, Arc<dyn CallableSpec<L>>>,
}

impl<L: Lang> FallbackProvider<L> {
    /// An empty fallback provider.
    pub fn new(name: impl Into<Box<str>>) -> FallbackProvider<L> {
        FallbackProvider { name: name.into(), fallbacks: BTreeMap::new() }
    }

    /// Register the fallback spec answering every name of `callable_type` (typically a
    /// shared singleton). Returns the previously registered fallback, if any.
    pub fn set(
        &mut self,
        callable_type: L::CallableTypeId,
        spec: Arc<dyn CallableSpec<L>>,
    ) -> Option<Arc<dyn CallableSpec<L>>> {
        self.fallbacks.insert(callable_type, spec)
    }

    /// The registered fallback for `callable_type`, if any.
    pub fn get(&self, callable_type: L::CallableTypeId) -> Option<&Arc<dyn CallableSpec<L>>> {
        self.fallbacks.get(&callable_type)
    }

    /// Every registered fallback — `(callable type, spec)` — ordered by callable type
    /// (the storage order; deterministic).
    pub fn fallbacks(&self) -> impl Iterator<Item = (L::CallableTypeId, &Arc<dyn CallableSpec<L>>)> + '_ {
        self.fallbacks.iter().map(|(callable_type, spec)| (*callable_type, spec))
    }
}

// The `SerializableObject` (the fallbacks in full) and `DeserializableObject` impls
// live in `crate::serialize::drivers::specs`.

impl<L: Lang> SpecsProvider<L> for FallbackProvider<L> {
    fn name(&self) -> &str {
        &self.name
    }

    fn retrieve_spec(
        &self,
        query: &CallableQuery<'_, L>,
        _state: &ParsingState<L>,
    ) -> Result<Option<Arc<dyn CallableSpec<L>>>, ProviderError> {
        Ok(self.fallbacks.get(&query.callable_type).cloned())
    }
}

impl<L: Lang> Clone for FallbackProvider<L> {
    fn clone(&self) -> Self {
        FallbackProvider { name: self.name.clone(), fallbacks: self.fallbacks.clone() }
    }
}

impl<L: Lang> fmt::Debug for FallbackProvider<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FallbackProvider")
            .field("name", &self.name)
            .field("callable_types", &self.fallbacks.len())
            .finish()
    }
}

// --- ErrorCallableSpec ---------------------------------------------------------------------

/// Condition: a callable that is *defined to be an error* was invoked — the
/// [`ErrorCallableSpec`] mechanism ("undefined on purpose" as an ordinary definition). The invocation recovers as a span-backed chars fallback.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.specs.callable-defined-as-error")]
pub struct CallableDefinedAsError {
    /// The invocation spelling, as written.
    pub name: String,
    /// Optional detail configured on the spec — *why* this name is an error ("removed
    /// by {package}", "not available in this mode").
    pub detail: Option<String>,
}

// Hand-written wording: the detail suffix is conditional (a branch the derive's message
// format string cannot express).
impl fmt::Display for CallableDefinedAsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "‘{}’ is defined to be an error", self.name)?;
        if let Some(detail) = &self.detail {
            write!(f, ": {detail}")?;
        }
        Ok(())
    }
}

/// A [`CallableSpec`] whose invocation is an error: the core-provided utility behind
/// "defined to be an error" (no `Masked` resolution outcome exists — shadowing lower
/// entries *and* the fallback with this spec suppresses them purely by search order,
/// with a better message than a mask could carry).
///
/// Its invocation parser records a [`CallableDefinedAsError`] through the recovery
/// entry point (strict: abort; tolerant: diagnostic) and stages the trigger as a span-backed chars
/// fallback, consuming nothing further.
#[derive(Debug, Clone, Default)]
pub struct ErrorCallableSpec {
    /// Optional detail for the diagnostic — *why* the name is an error.
    pub detail: Option<Box<str>>,
}

impl ErrorCallableSpec {
    /// An error spec with no detail beyond the standard message.
    pub fn new() -> ErrorCallableSpec {
        ErrorCallableSpec { detail: None }
    }

    /// An error spec whose diagnostic carries `detail`.
    pub fn with_detail(detail: impl Into<Box<str>>) -> ErrorCallableSpec {
        ErrorCallableSpec { detail: Some(detail.into()) }
    }
}

// The `SerializableObject` (a self-contained form: the detail) and
// `DeserializableObject` impls live in `crate::serialize::drivers::specs`.

impl<L: Lang> CallableSpec<L> for ErrorCallableSpec {
    /// Infallible: `Ok(...)` wrapping is this implementation's whole use of the
    /// `Result`.
    fn make_invocation_parser<'a, 's>(
        &'a self,
        invocation: Invocation<'a, 's, L>,
    ) -> Result<
        Box<dyn ConstructParser<L, Output = BuildId> + 'a>,
        crate::error::ParseError<L::SourceOrigin>,
    >
    where
        's: 'a,
    {
        Ok(Box::new(ErrorInvocationParser { invocation, detail: self.detail.as_deref() }))
    }
}

/// The invocation parser of [`ErrorCallableSpec`]: diagnose, stage the trigger as chars,
/// consume nothing further (the dispatch arm already consumed the trigger token whole).
struct ErrorInvocationParser<'a, 's, L: Lang> {
    invocation: Invocation<'a, 's, L>,
    detail: Option<&'a str>,
}

impl<L: Lang> ConstructParser<L> for ErrorInvocationParser<'_, '_, L> {
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (BuildId, Option<Box<ParsingStateDelta<L>>>)> {
        let span = cx.tokens.source_span_of(self.invocation.token);
        cx.recover(
            CallableDefinedAsError {
                name: self.invocation.name.into(),
                detail: self.detail.map(String::from),
            },
            span.clone(),
        )?;
        // Tolerant continuation: markup in a Chars node is the accepted recovery
        // artifact (DESIGN_RATIONALE.md [§dd-dr:errors]).
        let id = cx
            .stage_node(
                NodeKind::chars(span.span()),
                span.clone(),
                Arc::clone(&cx.state),
                Vec::new(),
            )
            .map_err(|error| cx.staging_error(error, span))?;
        Ok((id, None))
    }
}

// --- ScopeStack ---------------------------------------------------------------------------

/// The ordered provider stack stored in the parsing state
/// ([`StateData::scopes`](crate::state::StateData)): resolution and specials fold over
/// `Arc<dyn SpecsProvider<L>>` entries, **innermost (last-pushed) first** — lexical
/// shadowing (`\newcommand` semantics; no `ConflictStrategy`).
///
/// The stack is deliberately **not** a [`SpecsProvider`] (stacks do not nest — the
/// nested-fallback-preemption hazard is removed rather than re-mitigated) and
/// carries **no** fallback map: unknown-callable policy is an ordinary bottom
/// [`FallbackProvider`]. It is also visibility-blind — a mode-restricted [`Package`]
/// declines inside its own methods.
///
/// Mid-parse changes go through [`ScopeOp`]s in a state delta; [`apply_op`](ScopeStack::apply_op)
/// is the primitive behind them (also usable directly on an owned stack, e.g. while
/// seeding an initial state).
///
/// **Storage follows the language's feature declarations.** Like the token-rules
/// blocks, the provider list is stored through the scopes presence declaration of
/// [`Lang::Features`] ([`FeaturePresence::Store`]): for a language that declares the
/// scope stack absent, the list is replaced by the zero-sized store and the stack is
/// permanently empty — every lookup gives the empty-stack answer,
/// [`push`](ScopeStack::push) requires a language with the feature
/// ([`LangHasScopes`]), and [`apply_op`](ScopeStack::apply_op) reports
/// [`ScopeOpError::ScopesAbsent`].
pub struct ScopeStack<L: Lang> {
    /// Outermost first; folds iterate in reverse. For a language that declares the
    /// scope stack absent, this is the zero-sized store — no list exists.
    // The store projection is the point of the field; an alias would hide the very
    // indirection this declaration exists to state.
    #[allow(clippy::type_complexity)]
    stack: <<L::Features as LangFeatures>::Scopes as FeaturePresence>::Store<
        Vec<Arc<dyn SpecsProvider<L>>>,
    >,
}

impl<L: Lang> ScopeStack<L> {
    /// An empty stack: no definitions at all. Every language constructs one (the
    /// parsing state stores a stack unconditionally); for a language that declares
    /// the scope stack absent, the empty stack is the only value the stack ever has.
    pub fn new() -> ScopeStack<L> {
        ScopeStack { stack: <L::Features as LangFeatures>::Scopes::store_with(Vec::new) }
    }

    /// A stack over `providers` (outermost first), for any language: the
    /// deserialization path, which rebuilds a stack from serialized data under a
    /// generic `L` and so cannot use [`push`](ScopeStack::push) (bounded on the
    /// feature). `None` when the language declares the scope stack absent and
    /// `providers` is non-empty — such a stack cannot exist; an empty list yields the
    /// empty stack for every language.
    pub(crate) fn from_providers(providers: Vec<Arc<dyn SpecsProvider<L>>>) -> Option<ScopeStack<L>> {
        if !<L::Features as LangFeatures>::Scopes::PRESENT && !providers.is_empty() {
            return None;
        }
        Some(ScopeStack { stack: <L::Features as LangFeatures>::Scopes::store_with(|| providers) })
    }

    /// The provider list through the storage projection: the stored entries, or the
    /// empty slice when the language declares the scope stack absent (the stack is
    /// then permanently empty). Every read-only method routes through this, so each
    /// gives its empty-stack answer under an absent declaration.
    fn entries(&self) -> &[Arc<dyn SpecsProvider<L>>] {
        <L::Features as LangFeatures>::Scopes::store_get(&self.stack)
            .map_or(&[], |providers| providers.as_slice())
    }

    /// Push a provider as the new innermost entry. Requires a language whose
    /// features declare the scope stack present ([`LangHasScopes`]).
    pub fn push(&mut self, provider: Arc<dyn SpecsProvider<L>>)
    where
        L: LangHasScopes,
    {
        // Under the bound the store is transparent: the field is the list itself.
        self.stack.push(provider);
    }

    /// The providers, outermost first (the storage order; folds run innermost-first).
    /// The empty slice when the language declares the scope stack absent.
    pub fn providers(&self) -> &[Arc<dyn SpecsProvider<L>>] {
        self.entries()
    }

    /// The provider names, **innermost first** (the search order — the order the miss
    /// detail reports).
    pub fn provider_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.entries().iter().rev().map(|provider| provider.name())
    }

    /// A [`Display`](fmt::Display) adapter rendering the searched-provider detail for
    /// resolution misses (the `UnresolvableCommand` "searched: …" detail): exhausting
    /// the stack always searched *every* provider (the stack is visibility-blind), so
    /// the searched set is a property of the stack itself.
    pub fn searched_providers(&self) -> SearchedProviders<'_, L> {
        SearchedProviders { stack: self }
    }

    /// Number of providers on the stack.
    pub fn len(&self) -> usize {
        self.entries().len()
    }

    /// Whether the stack holds no providers.
    pub fn is_empty(&self) -> bool {
        self.entries().is_empty()
    }

    /// Resolve `query`: fold over the providers innermost-first; the first hit wins.
    /// `Ok(None)` is the structured miss — the whole stack was searched (compose the
    /// diagnostic detail via [`searched_providers`](ScopeStack::searched_providers)).
    /// A provider `Err` aborts the fold and propagates with the provider's name
    /// attached.
    pub fn retrieve_spec(
        &self,
        query: &CallableQuery<'_, L>,
        state: &ParsingState<L>,
    ) -> Result<Option<Arc<dyn CallableSpec<L>>>, ScopeStackError> {
        for provider in self.entries().iter().rev() {
            match provider.retrieve_spec(query, state) {
                Ok(Some(spec)) => return Ok(Some(spec)),
                Ok(None) => {}
                Err(error) => {
                    return Err(ScopeStackError { provider: provider.name().into(), error })
                }
            }
        }
        Ok(None)
    }

    /// The specials fold (the standard body of a preset's
    /// [`Lang::scan_specials`]): consult **every**
    /// provider innermost-first; the **longest** match wins, ties go **innermost**
    /// (pylatexenc `test_for_specials` parity — `---` beats `--` across providers, and
    /// an equal-length match is the same spelling, so the tie rule implements
    /// shadowing). A provider's scan `Err` aborts the fold and propagates.
    /// `pos` is passed through to every provider unchecked, under the `pos`
    /// contract documented on [`SpecsProvider::scan_specials`] (within
    /// `content`'s bounds, on a character boundary).
    pub fn scan_specials(
        &self,
        state: &ParsingState<L>,
        content: &str,
        pos: usize,
    ) -> Result<Option<SpecialsMatch<L>>, SpecialsScanError> {
        let mut best: Option<SpecialsMatch<L>> = None;
        for provider in self.entries().iter().rev() {
            if let Some(matched) = provider.scan_specials(state, content, pos)? {
                // Strictly longer wins; an equal-length later (outer) match loses.
                if best.as_ref().is_none_or(|current| matched.end > current.end) {
                    best = Some(matched);
                }
            }
        }
        Ok(best)
    }

    /// The trigger-character union over all providers (the standard body of a preset's
    /// [`Lang::specials_trigger_chars`]) —
    /// computed at state freeze and cached on the state, like every trigger filter.
    pub fn specials_trigger_chars(&self) -> TriggerChars {
        let mut union = TriggerChars::default();
        for provider in self.entries() {
            union = union.union(&provider.specials_trigger_chars());
        }
        union
    }

    /// Enumerate the definitions of the invocation form `callable_type` in scope under
    /// `mode`, **shadowing applied**: providers are consulted innermost-first (each
    /// pre-filters by visibility, so this mirrors [`retrieve_spec`](ScopeStack::retrieve_spec)
    /// resolution under that mode), and the first occurrence of a name wins. For
    /// specials definitions the "name" is the trigger spelling — identical triggers
    /// shadow innermost-wins, exactly the [`scan_specials`](ScopeStack::scan_specials)
    /// tie rule; distinct triggers coexist (longest-match is positional resolution, not
    /// shadowing).
    ///
    /// Providers that cannot enumerate ([`SpecsProvider::iter_symbols`] = `None`, e.g.
    /// a [`FallbackProvider`] answering *any* name) are skipped: the listing covers the
    /// enumerable definitions, not the open-ended fallback behavior. For the raw
    /// per-provider walk without dedup, iterate [`providers`](ScopeStack::providers)
    /// yourself. To enumerate *every* form, drive this once per type — via
    /// [`ClosedVocabulary::ALL`](crate::state::ClosedVocabulary) where the language's
    /// type vocabulary implements it.
    pub fn iter_symbols(
        &self,
        callable_type: L::CallableTypeId,
        mode: L::ModeId,
    ) -> Vec<SymbolEntry<'_, L>> {
        let mut seen: hashbrown::HashSet<&str> = hashbrown::HashSet::new();
        let mut symbols = Vec::new();
        for provider in self.entries().iter().rev() {
            let Some(entries) = provider.iter_symbols(callable_type, mode) else {
                continue;
            };
            for entry in entries {
                if seen.insert(entry.name) {
                    symbols.push(entry);
                }
            }
        }
        symbols
    }

    /// Apply one [`ScopeOp`] — the primitive behind
    /// [`ParsingStateDelta::scope_ops`](crate::state::ParsingStateDelta). Name-targeted
    /// ops address the innermost provider with that name; failures are reported, never
    /// silent (see the per-variant docs on [`ScopeOp`]). On a stack of a language that
    /// declares the scope stack absent, every op fails with
    /// [`ScopeOpError::ScopesAbsent`] — such a call can only be direct: a state delta
    /// cannot carry scope ops for that language.
    pub fn apply_op(&mut self, op: &ScopeOp<L>) -> Result<(), ScopeOpError> {
        let Some(stack) =
            <L::Features as LangFeatures>::Scopes::store_get_mut(&mut self.stack)
        else {
            return Err(ScopeOpError::ScopesAbsent);
        };
        match op {
            ScopeOp::Push(provider) => {
                stack.push(Arc::clone(provider));
                Ok(())
            }
            ScopeOp::Unload { name } => {
                let index = Self::innermost_position(stack, name).ok_or_else(|| {
                    ScopeOpError::UnknownProvider { name: name.clone() }
                })?;
                stack.remove(index);
                Ok(())
            }
            ScopeOp::Replace { name, provider } => {
                let index = Self::innermost_position(stack, name).ok_or_else(|| {
                    ScopeOpError::UnknownProvider { name: name.clone() }
                })?;
                stack[index] = Arc::clone(provider);
                Ok(())
            }
            ScopeOp::ReplaceStack(providers) => {
                *stack = providers.clone();
                Ok(())
            }
            ScopeOp::Define { scope, callable_type, name, spec } => {
                let op = DefinitionOp::Define {
                    callable_type: *callable_type,
                    name: name.clone(),
                    spec: Arc::clone(spec),
                };
                match Self::innermost_position(stack, scope) {
                    Some(index) => Self::route_definition(stack, index, scope, op),
                    None => {
                        // Lazy creation: the first Define into an absent scope name
                        // creates it, innermost (DESIGN_RATIONALE.md [§dd-dr:specs]).
                        let mut fresh = Scope::new(scope.clone());
                        fresh.insert(*callable_type, name.clone(), Arc::clone(spec));
                        stack.push(Arc::new(fresh));
                        Ok(())
                    }
                }
            }
            ScopeOp::Remove { scope, callable_type, name } => {
                let op = DefinitionOp::Remove {
                    callable_type: *callable_type,
                    name: name.clone(),
                };
                let index = Self::innermost_position(stack, scope).ok_or_else(|| {
                    ScopeOpError::UnknownProvider { name: scope.clone() }
                })?;
                Self::route_definition(stack, index, scope, op)
            }
        }
    }

    /// Route one definition op to the provider at `index` (copy-on-write entry swap).
    fn route_definition(
        stack: &mut [Arc<dyn SpecsProvider<L>>],
        index: usize,
        scope: &str,
        op: DefinitionOp<L>,
    ) -> Result<(), ScopeOpError> {
        match stack[index].with_definitions(&[op]) {
            Ok(fresh) => {
                stack[index] = fresh;
                Ok(())
            }
            Err(error) => Err(ScopeOpError::Provider(ScopeStackError {
                provider: scope.into(),
                error,
            })),
        }
    }

    /// Index of the innermost provider named `name`, if any.
    fn innermost_position(
        stack: &[Arc<dyn SpecsProvider<L>>],
        name: &str,
    ) -> Option<usize> {
        stack.iter().rposition(|provider| provider.name() == name)
    }
}

impl<L: Lang> Default for ScopeStack<L> {
    fn default() -> Self {
        ScopeStack::new()
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug`; cloning is cheap (`Arc`s).

impl<L: Lang> Clone for ScopeStack<L> {
    fn clone(&self) -> Self {
        ScopeStack { stack: self.stack.clone() }
    }
}

impl<L: Lang> fmt::Debug for ScopeStack<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeStack").field("providers", &self.stack).finish()
    }
}

/// [`Display`](fmt::Display) adapter over a stack's searched providers — see
/// [`ScopeStack::searched_providers`].
pub struct SearchedProviders<'a, L: Lang> {
    stack: &'a ScopeStack<L>,
}

impl<L: Lang> fmt::Display for SearchedProviders<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.stack.is_empty() {
            return write!(f, "no providers on the scope stack");
        }
        write!(f, "searched providers: ")?;
        for (i, name) in self.stack.provider_names().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{name}")?;
        }
        Ok(())
    }
}

impl<L: Lang> fmt::Debug for SearchedProviders<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::StdCallableSpec;
    use crate::state::StateData;
    use crate::token::{
        CommandRules, CommentRules, ForbiddenCharsRules, GroupRules, ParagraphRules,
        SpecialsRules, TokenErrorKind, TokenRules, WhitespaceRules,
    };
    use alloc::string::{String, ToString};
    use alloc::vec;

    const MACRO: u32 = 0;
    const ENVIRONMENT: u32 = 1;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl crate::state::TrivialLang for PlainLang {}

    // `Features = AllLangFeatures` (all test languages here declare it): the plain
    // block literals below only typecheck once the per-feature stores normalize to
    // the blocks themselves.
    fn min_rules<L: Lang<Features = crate::state::AllLangFeatures>>() -> TokenRules<L> {
        TokenRules {
            whitespace: WhitespaceRules::empty(),
            paragraphs: ParagraphRules { enabled: false },
            groups: GroupRules {
                enabled: true,
                rules: vec![],
                temporary: vec![],
                expecting_close: None,
            },
            commands: CommandRules {
                enabled: true,
                rules: vec![],
            },
            comments: CommentRules {
                enabled: true,
                rules: vec![],
            },
            specials: SpecialsRules { enabled: true },
            forbidden_chars: ForbiddenCharsRules { chars: "".into() },
        }
    }

    fn state_with(scopes: ScopeStack<PlainLang>) -> ParsingState<PlainLang> {
        ParsingState::new(StateData { rules: min_rules(), scopes, mode: (), ext: () })
    }

    fn new_spec<L: Lang>() -> Arc<dyn CallableSpec<L>> {
        Arc::new(StdCallableSpec::default())
    }

    fn macro_query(name: &str) -> CallableQuery<'_, PlainLang> {
        CallableQuery::new(MACRO, name, CallableSyntax::Command { escape_char: '\\' })
    }

    fn package_with(name: &str, definitions: &[(&str, &Arc<dyn CallableSpec<PlainLang>>)]) -> Package<PlainLang> {
        let mut package = Package::new(name);
        for (defined, spec) in definitions {
            package.insert(MACRO, *defined, Arc::clone(spec));
        }
        package
    }

    // --- Package data surface -----------------------------------------------------------

    #[test]
    fn dyn_specs_provider_downcasts_to_its_concrete_type() {
        // The `Any` supertrait: a stored `Arc<dyn SpecsProvider<L>>` gives its concrete
        // type back through dyn-to-`Any` upcasting.
        let provider: Arc<dyn SpecsProvider<PlainLang>> = Arc::new(Package::new("mypkg"));
        let any: &dyn Any = &*provider;
        let package = any.downcast_ref::<Package<PlainLang>>().expect("downcast to Package");
        assert_eq!(package.name(), "mypkg");
    }

    #[test]
    fn package_insert_get_replace() {
        let mut package: Package<PlainLang> = Package::new("test");
        assert!(package.is_empty());

        let first = new_spec();
        let second = new_spec();
        assert!(package.insert(MACRO, "foo", first.clone()).is_none());
        assert!(Arc::ptr_eq(package.get(MACRO, "foo").unwrap(), &first));

        // Same key replaces (shadowing within one provider = replacement) and returns
        // the previous spec; other callable types are separate key spaces.
        let previous = package.insert(MACRO, "foo", second.clone()).unwrap();
        assert!(Arc::ptr_eq(&previous, &first));
        assert!(package.get(ENVIRONMENT, "foo").is_none());
        assert_eq!(package.len(), 1);
    }

    #[test]
    fn a_query_carries_the_triggering_tokens_view_or_nothing() {
        // `new` leaves the token context empty (specials scan, synthesized
        // invocations); `with_token_kind` attaches the trigger's view — which is all a
        // provider, having no reader, can know about the token.
        let query: CallableQuery<'_, PlainLang> =
            CallableQuery::new(MACRO, "emph", CallableSyntax::Command { escape_char: '\\' });
        assert!(query.token_kind.is_none());

        let view = TokenKindView::Command { name: "emph", escape_char: '\\' };
        let query = query.with_token_kind(view);
        assert_eq!(query.token_kind, Some(view));
        // The query stays `Copy`, view included.
        let copy = query;
        assert_eq!(copy.token_kind, query.token_kind);
        assert_eq!(copy.name, "emph");
    }

    #[test]
    fn specs_are_flyweights_across_names() {
        let mut package: Package<PlainLang> = Package::new("test");
        let shared = new_spec();
        package.insert(MACRO, "emph", shared.clone());
        package.insert(MACRO, "textit", shared.clone());

        let a = package.get(MACRO, "emph").unwrap();
        let b = package.get(MACRO, "textit").unwrap();
        assert!(Arc::ptr_eq(a, b));
    }

    // --- stack retrieval: shadowing, fallbacks, error specs, provider errors ------------

    #[test]
    fn stack_retrieval_innermost_wins() {
        let outer_spec = new_spec();
        let inner_spec = new_spec();

        let outer = package_with("outer", &[("foo", &outer_spec), ("bar", &outer_spec)]);
        let inner = package_with("inner", &[("foo", &inner_spec)]);

        let mut stack = ScopeStack::new();
        stack.push(Arc::new(outer));
        stack.push(Arc::new(inner));
        let st = state_with(ScopeStack::new());

        // "foo" shadowed by the innermost; "bar" falls through to the outer package;
        // a full miss is the structured Ok(None).
        let foo = stack.retrieve_spec(&macro_query("foo"), &st).unwrap().unwrap();
        assert!(Arc::ptr_eq(&foo, &inner_spec));
        let bar = stack.retrieve_spec(&macro_query("bar"), &st).unwrap().unwrap();
        assert!(Arc::ptr_eq(&bar, &outer_spec));
        assert!(stack.retrieve_spec(&macro_query("nope"), &st).unwrap().is_none());
    }

    #[test]
    fn fallback_provider_is_ordinary_bottom_of_stack_policy() {
        let defined = new_spec();
        let unknown_macro = new_spec();

        let mut fallbacks: FallbackProvider<PlainLang> = FallbackProvider::new("fallbacks");
        assert!(fallbacks.set(MACRO, unknown_macro.clone()).is_none());
        assert!(Arc::ptr_eq(fallbacks.get(MACRO).unwrap(), &unknown_macro));

        let mut stack = ScopeStack::new();
        stack.push(Arc::new(fallbacks)); // bottom: pushed first
        stack.push(Arc::new(package_with("outer", &[("known", &defined)])));
        let st = state_with(ScopeStack::new());

        // Stack hit above the fallback: the fallback never answers.
        let known = stack.retrieve_spec(&macro_query("known"), &st).unwrap().unwrap();
        assert!(Arc::ptr_eq(&known, &defined));

        // Miss above: the fallback answers *any* name of its type — same singleton.
        let a = stack.retrieve_spec(&macro_query("nope"), &st).unwrap().unwrap();
        let b = stack.retrieve_spec(&macro_query("also-nope"), &st).unwrap().unwrap();
        assert!(Arc::ptr_eq(&a, &unknown_macro));
        assert!(Arc::ptr_eq(&a, &b));

        // No fallback registered for ENVIRONMENT: a true miss.
        let env_query =
            CallableQuery::new(ENVIRONMENT, "nope", CallableSyntax::Command { escape_char: '\\' });
        assert!(stack.retrieve_spec(&env_query, &st).unwrap().is_none());
    }

    #[test]
    fn error_spec_shadowing_suppresses_lower_entries_and_fallback() {
        // "Defined to be an error" is an ordinary definition: shadowing with an
        // ErrorCallableSpec suppresses the package definition *and* the fallback purely
        // by search order (a theorem of ordering, not an extra rule).
        let real = new_spec();
        let error_spec: Arc<dyn CallableSpec<PlainLang>> =
            Arc::new(ErrorCallableSpec::with_detail("removed on purpose"));

        let mut fallbacks: FallbackProvider<PlainLang> = FallbackProvider::new("fallbacks");
        fallbacks.set(MACRO, new_spec());
        let mut shadow: Scope<PlainLang> = Scope::new("shadow");
        shadow.insert(MACRO, "gone", Arc::clone(&error_spec));

        let mut stack = ScopeStack::new();
        stack.push(Arc::new(fallbacks));
        stack.push(Arc::new(package_with("outer", &[("gone", &real)])));
        stack.push(Arc::new(shadow));
        let st = state_with(ScopeStack::new());

        let resolved = stack.retrieve_spec(&macro_query("gone"), &st).unwrap().unwrap();
        assert!(Arc::ptr_eq(&resolved, &error_spec));
    }

    #[test]
    fn provider_error_aborts_retrieval_with_the_providers_name() {
        #[derive(Debug)]
        struct Failing;
        impl SerializableObject<PlainLang> for Failing {}
        impl SpecsProvider<PlainLang> for Failing {
            fn name(&self) -> &str {
                "failing"
            }
            fn retrieve_spec(
                &self,
                _query: &CallableQuery<'_, PlainLang>,
                _state: &ParsingState<PlainLang>,
            ) -> Result<Option<Arc<dyn CallableSpec<PlainLang>>>, ProviderError> {
                Err(ProviderError::Failed("backing store unavailable".into()))
            }
        }

        let answer = new_spec();
        let mut stack = ScopeStack::new();
        stack.push(Arc::new(package_with("outer", &[("foo", &answer)])));
        stack.push(Arc::new(Failing)); // innermost: hit first, aborts the fold
        let st = state_with(ScopeStack::new());

        let error = stack.retrieve_spec(&macro_query("foo"), &st).unwrap_err();
        assert_eq!(&*error.provider, "failing");
        assert_eq!(error.error, ProviderError::Failed("backing store unavailable".into()));
        assert!(error.to_string().contains("failing"));
    }

    #[test]
    fn searched_providers_reports_innermost_first() {
        let mut stack: ScopeStack<PlainLang> = ScopeStack::new();
        assert_eq!(stack.searched_providers().to_string(), "no providers on the scope stack");

        stack.push(Arc::new(package_with("outer", &[])));
        stack.push(Arc::new(package_with("inner", &[])));
        assert_eq!(
            stack.searched_providers().to_string(),
            "searched providers: inner, outer"
        );
        assert_eq!(stack.provider_names().collect::<Vec<_>>(), vec!["inner", "outer"]);
    }

    // --- custom-provider dispatch (carried over from the Phase 4 lookup contract) --------

    #[test]
    fn provider_can_dispatch_on_syntax() {
        // `\foo` and `#foo` resolve to different specs (open question [§dd-dr:open-questions] (a), decided
        // July 2026; the query carries the fired escape char).
        #[derive(Debug)]
        struct BySyntax {
            backslash: Arc<dyn CallableSpec<PlainLang>>,
            hash: Arc<dyn CallableSpec<PlainLang>>,
        }
        impl SerializableObject<PlainLang> for BySyntax {}
        impl SpecsProvider<PlainLang> for BySyntax {
            fn name(&self) -> &str {
                "by-syntax"
            }
            fn retrieve_spec(
                &self,
                query: &CallableQuery<'_, PlainLang>,
                _state: &ParsingState<PlainLang>,
            ) -> Result<Option<Arc<dyn CallableSpec<PlainLang>>>, ProviderError> {
                Ok(match query.syntax {
                    CallableSyntax::Command { escape_char: '\\' } => Some(self.backslash.clone()),
                    CallableSyntax::Command { escape_char: '#' } => Some(self.hash.clone()),
                    _ => None,
                })
            }
        }

        let by_syntax = BySyntax { backslash: new_spec(), hash: new_spec() };
        let backslash_spec = by_syntax.backslash.clone();
        let hash_spec = by_syntax.hash.clone();

        let mut stack = ScopeStack::new();
        stack.push(Arc::new(by_syntax));
        let st = state_with(ScopeStack::new());

        let via_backslash = stack.retrieve_spec(&macro_query("foo"), &st).unwrap().unwrap();
        assert!(Arc::ptr_eq(&via_backslash, &backslash_spec));

        let hash_query =
            CallableQuery::new(MACRO, "foo", CallableSyntax::Command { escape_char: '#' });
        let via_hash = stack.retrieve_spec(&hash_query, &st).unwrap().unwrap();
        assert!(Arc::ptr_eq(&via_hash, &hash_spec));
    }

    // --- mode visibility ------------------------------------------------------------------

    #[derive(Debug, Clone, Copy, PartialEq, Eq, core::hash::Hash, Default)]
    enum Mode {
        #[default]
        Text,
        Math,
    }

    #[derive(Debug, Clone, Copy)]
    struct ModedLang;
    impl Lang for ModedLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = Mode;
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type StreamPosition = crate::token::StdStreamPosition;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    fn moded_state(scopes: ScopeStack<ModedLang>, mode: Mode) -> ParsingState<ModedLang> {
        ParsingState::new(StateData { rules: min_rules(), scopes, mode, ext: () })
    }

    #[test]
    fn package_mode_visibility_is_checked_by_the_package_itself() {
        let math_spec: Arc<dyn CallableSpec<ModedLang>> = new_spec();
        let text_spec: Arc<dyn CallableSpec<ModedLang>> = new_spec();

        let mut math_pkg: Package<ModedLang> = Package::new("math-only");
        math_pkg.insert(MACRO, "vec", Arc::clone(&math_spec));
        math_pkg.insert_specials(MACRO, "&", Arc::clone(&math_spec));
        math_pkg.set_visible_modes(Some(vec![Mode::Math]));

        let mut base: Package<ModedLang> = Package::new("allmodes");
        base.insert(MACRO, "vec", Arc::clone(&text_spec));

        let mut stack = ScopeStack::new();
        stack.push(Arc::new(base));
        stack.push(Arc::new(math_pkg)); // innermost, but mode-restricted
        let text_state = moded_state(stack.clone(), Mode::Text);
        let math_state = moded_state(stack.clone(), Mode::Math);

        let query: CallableQuery<'_, ModedLang> =
            CallableQuery::new(MACRO, "vec", CallableSyntax::Command { escape_char: '\\' });

        // Text mode: the restricted package answers "not here" — the stack is
        // visibility-blind and simply continues outward.
        let in_text = stack.retrieve_spec(&query, &text_state).unwrap().unwrap();
        assert!(Arc::ptr_eq(&in_text, &text_spec));
        // Math mode: the innermost package answers.
        let in_math = stack.retrieve_spec(&query, &math_state).unwrap().unwrap();
        assert!(Arc::ptr_eq(&in_math, &math_spec));

        // Specials scan respects the same visibility; the trigger-char union does NOT
        // (chars stay in the filter regardless of state — the scan declines instead).
        assert!(stack.scan_specials(&text_state, "&x", 0).unwrap().is_none());
        assert!(stack.scan_specials(&math_state, "&x", 0).unwrap().is_some());
        assert_eq!(stack.specials_trigger_chars(), TriggerChars::Only("&".into()));
    }

    #[test]
    fn per_entry_mode_visibility_gates_within_a_package() {
        let text_spec: Arc<dyn CallableSpec<ModedLang>> = new_spec();
        let any_spec: Arc<dyn CallableSpec<ModedLang>> = new_spec();

        // One package, no package-level restriction: a text-only entry and an all-modes
        // entry side by side — the fine gate (per-entry mode visibility).
        let mut pkg: Package<ModedLang> = Package::new("mixed");
        pkg.insert_in_modes(MACRO, "emph", Arc::clone(&text_spec), Some(vec![Mode::Text]));
        pkg.insert(MACRO, "vec", Arc::clone(&any_spec)); // all modes
        pkg.insert_specials_in_modes(MACRO, "--", Arc::clone(&text_spec), Some(vec![Mode::Text]));
        pkg.insert_specials(MACRO, "~", Arc::clone(&any_spec)); // all modes

        let mut stack = ScopeStack::new();
        stack.push(Arc::new(pkg));
        let text_state = moded_state(stack.clone(), Mode::Text);
        let math_state = moded_state(stack.clone(), Mode::Math);

        let emph: CallableQuery<'_, ModedLang> =
            CallableQuery::new(MACRO, "emph", CallableSyntax::Command { escape_char: '\\' });
        let vec_q: CallableQuery<'_, ModedLang> =
            CallableQuery::new(MACRO, "vec", CallableSyntax::Command { escape_char: '\\' });

        // The text-only definition resolves in text, not in math; the all-modes one in
        // both.
        assert!(stack.retrieve_spec(&emph, &text_state).unwrap().is_some());
        assert!(stack.retrieve_spec(&emph, &math_state).unwrap().is_none());
        assert!(stack.retrieve_spec(&vec_q, &text_state).unwrap().is_some());
        assert!(stack.retrieve_spec(&vec_q, &math_state).unwrap().is_some());

        // Specials obey the same per-entry gate; the trigger-char union stays mode-blind.
        assert!(stack.scan_specials(&text_state, "--x", 0).unwrap().is_some());
        assert!(stack.scan_specials(&math_state, "--x", 0).unwrap().is_none());
        assert!(stack.scan_specials(&text_state, "~x", 0).unwrap().is_some());
        assert!(stack.scan_specials(&math_state, "~x", 0).unwrap().is_some());
    }

    // --- Scope: copy-on-write definition updates ------------------------------------------

    #[test]
    fn scope_with_definitions_is_functional_and_atomic() {
        let original_spec = new_spec();
        let mut scope: Scope<PlainLang> = Scope::new("user");
        scope.insert(MACRO, "foo", Arc::clone(&original_spec));
        let scope: Arc<dyn SpecsProvider<PlainLang>> = Arc::new(scope);
        let st = state_with(ScopeStack::new());

        // Define returns a fresh provider; the original is untouched (CoW).
        let added = new_spec();
        let fresh = scope
            .with_definitions(&[DefinitionOp::Define {
                callable_type: MACRO,
                name: "bar".into(),
                spec: Arc::clone(&added),
            }])
            .unwrap();
        assert!(fresh.retrieve_spec(&macro_query("bar"), &st).unwrap().is_some());
        assert!(scope.retrieve_spec(&macro_query("bar"), &st).unwrap().is_none());

        // Remove works on the fresh copy; removing an absent name is an error…
        let removed = fresh
            .with_definitions(&[DefinitionOp::Remove { callable_type: MACRO, name: "foo".into() }])
            .unwrap();
        assert!(removed.retrieve_spec(&macro_query("foo"), &st).unwrap().is_none());
        let error = removed
            .with_definitions(&[DefinitionOp::Remove { callable_type: MACRO, name: "foo".into() }])
            .unwrap_err();
        assert_eq!(error, ProviderError::UndefinedName { name: "foo".into() });

        // …and a failing op anywhere in the batch discards the whole batch (atomic).
        let error = scope
            .with_definitions(&[
                DefinitionOp::Define {
                    callable_type: MACRO,
                    name: "baz".into(),
                    spec: new_spec(),
                },
                DefinitionOp::Remove { callable_type: MACRO, name: "absent".into() },
            ])
            .unwrap_err();
        assert_eq!(error, ProviderError::UndefinedName { name: "absent".into() });
        assert!(scope.retrieve_spec(&macro_query("baz"), &st).unwrap().is_none());
    }

    #[test]
    fn immutable_providers_refuse_definition_ops() {
        let package: Arc<dyn SpecsProvider<PlainLang>> = Arc::new(package_with("pkg", &[]));
        let error = package
            .with_definitions(&[DefinitionOp::Remove { callable_type: MACRO, name: "x".into() }])
            .unwrap_err();
        assert_eq!(error, ProviderError::NotMutable);
    }

    // --- ScopeStack::apply_op ---------------------------------------------------------------

    #[test]
    fn apply_op_stack_shapes() {
        let mut stack: ScopeStack<PlainLang> = ScopeStack::new();
        let a: Arc<dyn SpecsProvider<PlainLang>> = Arc::new(package_with("a", &[]));
        let b: Arc<dyn SpecsProvider<PlainLang>> = Arc::new(package_with("b", &[]));
        let b2: Arc<dyn SpecsProvider<PlainLang>> = Arc::new(package_with("b", &[]));

        stack.apply_op(&ScopeOp::Push(Arc::clone(&a))).unwrap();
        stack.apply_op(&ScopeOp::Push(Arc::clone(&b))).unwrap();
        assert_eq!(stack.provider_names().collect::<Vec<_>>(), vec!["b", "a"]);

        // Replace swaps in place (innermost match).
        stack.apply_op(&ScopeOp::Replace { name: "b".into(), provider: Arc::clone(&b2) }).unwrap();
        assert!(Arc::ptr_eq(&stack.providers()[1], &b2));

        // Unload removes; unknown names are errors, never silent no-ops.
        stack.apply_op(&ScopeOp::Unload { name: "b".into() }).unwrap();
        assert_eq!(stack.len(), 1);
        let error = stack.apply_op(&ScopeOp::Unload { name: "b".into() }).unwrap_err();
        assert_eq!(error, ScopeOpError::UnknownProvider { name: "b".into() });
        let error = stack
            .apply_op(&ScopeOp::Replace { name: "zz".into(), provider: Arc::clone(&b2) })
            .unwrap_err();
        assert_eq!(error, ScopeOpError::UnknownProvider { name: "zz".into() });

        // ReplaceStack swaps wholesale.
        stack.apply_op(&ScopeOp::ReplaceStack(vec![Arc::clone(&b2)])).unwrap();
        assert_eq!(stack.provider_names().collect::<Vec<_>>(), vec!["b"]);
    }

    #[test]
    fn apply_op_define_routes_cow_and_lazily_creates() {
        let st = state_with(ScopeStack::new());
        let mut stack: ScopeStack<PlainLang> = ScopeStack::new();
        let mut user: Scope<PlainLang> = Scope::new("user");
        user.insert(MACRO, "foo", new_spec());
        stack.push(Arc::new(user));
        stack.push(Arc::new(package_with("pkg", &[])));
        let before = stack.clone(); // shares the Arcs — the structural-reversion stand-in

        // Define routed to the (non-innermost) named scope: CoW entry swap — the
        // cloned stack still holds the old provider and never sees the definition.
        let added = new_spec();
        stack
            .apply_op(&ScopeOp::Define {
                scope: "user".into(),
                callable_type: MACRO,
                name: "bar".into(),
                spec: Arc::clone(&added),
            })
            .unwrap();
        let bar = stack.retrieve_spec(&macro_query("bar"), &st).unwrap().unwrap();
        assert!(Arc::ptr_eq(&bar, &added));
        assert!(before.retrieve_spec(&macro_query("bar"), &st).unwrap().is_none());
        assert!(!Arc::ptr_eq(&stack.providers()[0], &before.providers()[0]));
        assert!(Arc::ptr_eq(&stack.providers()[1], &before.providers()[1]));

        // Define into an absent scope name creates it lazily, innermost.
        stack
            .apply_op(&ScopeOp::Define {
                scope: "local".into(),
                callable_type: MACRO,
                name: "baz".into(),
                spec: new_spec(),
            })
            .unwrap();
        assert_eq!(stack.provider_names().collect::<Vec<_>>(), vec!["local", "pkg", "user"]);
        assert!(stack.retrieve_spec(&macro_query("baz"), &st).unwrap().is_some());

        // Define routed to an immutable provider fails with the provider's refusal.
        let error = stack
            .apply_op(&ScopeOp::Define {
                scope: "pkg".into(),
                callable_type: MACRO,
                name: "x".into(),
                spec: new_spec(),
            })
            .unwrap_err();
        assert_eq!(
            error,
            ScopeOpError::Provider(ScopeStackError {
                provider: "pkg".into(),
                error: ProviderError::NotMutable,
            })
        );

        // Remove routes the same way; an absent *provider* is UnknownProvider.
        stack
            .apply_op(&ScopeOp::Remove {
                scope: "user".into(),
                callable_type: MACRO,
                name: "bar".into(),
            })
            .unwrap();
        assert!(stack.retrieve_spec(&macro_query("bar"), &st).unwrap().is_none());
        let error = stack
            .apply_op(&ScopeOp::Remove {
                scope: "nowhere".into(),
                callable_type: MACRO,
                name: "bar".into(),
            })
            .unwrap_err();
        assert_eq!(error, ScopeOpError::UnknownProvider { name: "nowhere".into() });
    }

    // --- specials: package data, the stack fold, trigger unions ---------------------------

    fn specials_package(
        name: &str,
        triggers: &[(&str, &Arc<dyn CallableSpec<PlainLang>>)],
    ) -> Package<PlainLang> {
        let mut package = Package::new(name);
        for (trigger, spec) in triggers {
            package.insert_specials(MACRO, *trigger, Arc::clone(spec));
        }
        package
    }

    #[test]
    fn package_specials_match_longest_within_the_package() {
        let short = new_spec();
        let long = new_spec();
        let mut package = specials_package("lig", &[("--", &short), ("---", &long)]);
        let st = state_with(ScopeStack::new());

        // The match reports its end; the matched text `content[pos..end]` is the name.
        let matched = package.scan_specials(&st, "---x", 0).unwrap().unwrap();
        assert_eq!(matched.end, 3);
        assert!(Arc::ptr_eq(&matched.spec, &long));

        let matched = package.scan_specials(&st, "--x", 0).unwrap().unwrap();
        assert_eq!(matched.end, 2);
        assert!(package.scan_specials(&st, "x--", 0).unwrap().is_none());

        // Positions are absolute; matching mid-content works on byte offsets.
        let matched = package.scan_specials(&st, "a--b", 1).unwrap().unwrap();
        assert_eq!(matched.end, 3);

        // Re-inserting an existing trigger replaces its resolution.
        let replacement = new_spec();
        let previous = package.insert_specials(MACRO, "---", Arc::clone(&replacement)).unwrap();
        assert!(Arc::ptr_eq(&previous, &long));
        let matched = package.scan_specials(&st, "---", 0).unwrap().unwrap();
        assert!(Arc::ptr_eq(&matched.spec, &replacement));

        assert_eq!(package.specials_trigger_chars(), TriggerChars::Only("-".into()));
    }

    /// The `pos` contract: an invalid position (out of the content's bounds or
    /// not a character boundary) answers an implementation-error `SpecialsScanError`
    /// — never a panic, never a silent no-match — while every valid boundary
    /// position scans as before.
    #[test]
    fn package_scan_specials_rejects_invalid_positions_with_an_error() {
        let spec = new_spec();
        let package = specials_package("lig", &[("--", &spec)]);
        let st = state_with(ScopeStack::new());

        // Out of range — the message names the case.
        let error = package.scan_specials(&st, "--", 7).unwrap_err();
        assert!(error.kind.to_string().contains("extension contract violation"));
        assert!(error.kind.to_string().contains("invalid position 7"));
        assert!(
            error.kind.to_string().contains("out of the content's bounds"),
            "{:?}",
            error
        );

        // Mid-character: `é` is two bytes, so pos 1 is not a character boundary —
        // named as the boundary case, not the bounds case.
        let error = package.scan_specials(&st, "é--", 1).unwrap_err();
        assert!(error.kind.to_string().contains("invalid position 1"));
        assert!(
            error.kind.to_string().contains("not a character boundary"),
            "{:?}",
            error
        );

        // Valid boundary positions are unaffected — end of content included.
        assert!(package.scan_specials(&st, "x--", 1).unwrap().is_some());
        assert!(package.scan_specials(&st, "--", 2).unwrap().is_none());
        assert!(package.scan_specials(&st, "é--", 2).unwrap().is_some());
    }

    #[test]
    fn stack_specials_fold_longest_wins_ties_innermost() {
        let inner_short = new_spec();
        let outer_long = new_spec();
        let outer_short = new_spec();

        // Inner provider defines `--`; outer defines `--` AND `---`.
        let inner = specials_package("inner", &[("--", &inner_short)]);
        let outer = specials_package("outer", &[("--", &outer_short), ("---", &outer_long)]);
        let mut stack = ScopeStack::new();
        stack.push(Arc::new(outer));
        stack.push(Arc::new(inner));
        let st = state_with(ScopeStack::new());

        // Longest wins across providers: `---` (outer) beats `--` (inner) — the
        // pylatexenc parity case.
        let matched = stack.scan_specials(&st, "---", 0).unwrap().unwrap();
        assert_eq!(matched.end, 3);
        assert!(Arc::ptr_eq(&matched.spec, &outer_long));

        // Equal length is the same spelling: the tie goes innermost — shadowing.
        let matched = stack.scan_specials(&st, "--x", 0).unwrap().unwrap();
        assert!(Arc::ptr_eq(&matched.spec, &inner_short));

        // Union of trigger chars over providers, deduplicated.
        assert_eq!(stack.specials_trigger_chars(), TriggerChars::Only("-".into()));
    }

    #[test]
    fn stack_specials_fold_propagates_provider_scan_errors() {
        #[derive(Debug)]
        struct FailingScan;
        impl SerializableObject<PlainLang> for FailingScan {}
        impl SpecsProvider<PlainLang> for FailingScan {
            fn name(&self) -> &str {
                "failing-scan"
            }
            fn retrieve_spec(
                &self,
                _query: &CallableQuery<'_, PlainLang>,
                _state: &ParsingState<PlainLang>,
            ) -> Result<Option<Arc<dyn CallableSpec<PlainLang>>>, ProviderError> {
                Ok(None)
            }
            fn scan_specials(
                &self,
                _state: &ParsingState<PlainLang>,
                _content: &str,
                _pos: usize,
            ) -> Result<Option<SpecialsMatch<PlainLang>>, SpecialsScanError> {
                Err(SpecialsScanError {
                    kind: TokenErrorKind::Custom(Box::new(
                        crate::constructs::ImplementationError::new("scan broke".to_string()),
                    )),
                    span: crate::source::Span::new(0, 1),
                })
            }
            fn specials_trigger_chars(&self) -> TriggerChars {
                TriggerChars::Any
            }
        }

        let ok = new_spec();
        let mut stack = ScopeStack::new();
        stack.push(Arc::new(specials_package("outer", &[("-", &ok)])));
        stack.push(Arc::new(FailingScan));
        let st = state_with(ScopeStack::new());

        let error = stack.scan_specials(&st, "-", 0).unwrap_err();
        assert!(error.kind.to_string().contains("scan broke"));
        assert_eq!(stack.specials_trigger_chars(), TriggerChars::Any);
    }

    // --- iter_symbols (Phase 7.8) -----------------------------------------------------

    fn names_of<'a, L: Lang>(symbols: &[SymbolEntry<'a, L>]) -> Vec<&'a str> {
        symbols.iter().map(|entry| entry.name).collect()
    }

    #[test]
    fn package_enumerates_by_callable_type_including_specials() {
        let spec = new_spec();
        let mut pkg: Package<PlainLang> = Package::new("p");
        pkg.insert(MACRO, "alpha", Arc::clone(&spec));
        pkg.insert(ENVIRONMENT, "itemize", Arc::clone(&spec));
        pkg.insert_specials(MACRO, "--", Arc::clone(&spec));

        let mut macros = names_of(&pkg.iter_symbols(MACRO, ()).unwrap().collect::<Vec<_>>());
        macros.sort_unstable();
        // The by-name table and the specials table both contribute under the entries'
        // *recorded* type; the specials row's name is the trigger spelling.
        assert_eq!(macros, ["--", "alpha"]);
        let environments =
            names_of(&pkg.iter_symbols(ENVIRONMENT, ()).unwrap().collect::<Vec<_>>());
        assert_eq!(environments, ["itemize"]);
        // Rows are self-describing and borrow the stored spec.
        let row = pkg.iter_symbols(ENVIRONMENT, ()).unwrap().next().unwrap();
        assert_eq!(row.callable_type, ENVIRONMENT);
        assert!(Arc::ptr_eq(row.spec, &spec));
    }

    #[test]
    fn package_iter_symbols_applies_both_visibility_gates() {
        let spec: Arc<dyn CallableSpec<ModedLang>> = new_spec();
        let mut pkg: Package<ModedLang> = Package::new("mixed");
        pkg.insert(MACRO, "vec", Arc::clone(&spec)); // all modes
        pkg.insert_in_modes(MACRO, "emph", Arc::clone(&spec), Some(vec![Mode::Text]));
        pkg.insert_specials_in_modes(MACRO, "--", Arc::clone(&spec), Some(vec![Mode::Text]));

        let mut in_text = names_of(&pkg.iter_symbols(MACRO, Mode::Text).unwrap().collect::<Vec<_>>());
        in_text.sort_unstable();
        assert_eq!(in_text, ["--", "emph", "vec"]);
        // The per-entry gate filters; the all-modes entry stays.
        assert_eq!(
            names_of(&pkg.iter_symbols(MACRO, Mode::Math).unwrap().collect::<Vec<_>>()),
            ["vec"]
        );

        // The package-level gate: enumerable, nothing visible (Some(empty), not None).
        let mut gated: Package<ModedLang> = Package::new("math-only");
        gated.insert(MACRO, "frac", new_spec());
        gated.set_visible_modes(Some(vec![Mode::Math]));
        assert_eq!(gated.iter_symbols(MACRO, Mode::Text).unwrap().count(), 0);
        assert_eq!(gated.iter_symbols(MACRO, Mode::Math).unwrap().count(), 1);
    }

    #[test]
    fn scope_enumerates_ignoring_mode() {
        let mut scope: Scope<ModedLang> = Scope::new("local");
        scope.insert(MACRO, "beta", new_spec());
        scope.insert(ENVIRONMENT, "quote", new_spec());
        // Scope definitions carry no visibility: every mode sees them.
        assert_eq!(names_of(&scope.iter_symbols(MACRO, Mode::Math).unwrap().collect::<Vec<_>>()), ["beta"]);
        assert_eq!(names_of(&scope.iter_symbols(MACRO, Mode::Text).unwrap().collect::<Vec<_>>()), ["beta"]);
        assert_eq!(
            names_of(&scope.iter_symbols(ENVIRONMENT, Mode::Text).unwrap().collect::<Vec<_>>()),
            ["quote"]
        );
    }

    #[test]
    fn default_iter_symbols_is_not_enumerable() {
        // FallbackProvider answers *any* name — it keeps the trait default.
        let mut fallback: FallbackProvider<PlainLang> = FallbackProvider::new("fallback");
        fallback.set(MACRO, new_spec());
        assert!(fallback.iter_symbols(MACRO, ()).is_none());
    }

    #[test]
    fn stack_iter_symbols_dedups_innermost_first_and_skips_unenumerable() {
        let outer_spec = new_spec();
        let inner_spec = new_spec();
        let mut outer: Package<PlainLang> = Package::new("outer");
        outer.insert(MACRO, "alpha", Arc::clone(&outer_spec));
        outer.insert(MACRO, "omega", Arc::clone(&outer_spec));
        let mut inner: Scope<PlainLang> = Scope::new("inner");
        inner.insert(MACRO, "alpha", Arc::clone(&inner_spec)); // shadows outer's

        let mut fallback: FallbackProvider<PlainLang> = FallbackProvider::new("fallback");
        fallback.set(MACRO, new_spec());

        let mut stack: ScopeStack<PlainLang> = ScopeStack::new();
        stack.push(Arc::new(fallback)); // unenumerable: skipped, not an error
        stack.push(Arc::new(outer));
        stack.push(Arc::new(inner));

        let symbols = stack.iter_symbols(MACRO, ());
        let mut names = names_of(&symbols);
        names.sort_unstable();
        assert_eq!(names, ["alpha", "omega"]);
        // First (innermost) visible occurrence wins — mirrors retrieve_spec resolution.
        let alpha = symbols.iter().find(|entry| entry.name == "alpha").unwrap();
        assert!(Arc::ptr_eq(alpha.spec, &inner_spec));
        // Innermost entries come first in the listing.
        assert_eq!(names_of(&symbols)[0], "alpha");

        // A mode-invisible provider's entries fall through to outer ones: the dedup is
        // "first *visible* wins", because each provider pre-filters by mode.
        let moded_spec: Arc<dyn CallableSpec<ModedLang>> = new_spec();
        let text_spec: Arc<dyn CallableSpec<ModedLang>> = new_spec();
        let mut math_pkg: Package<ModedLang> = Package::new("math-only");
        math_pkg.insert(MACRO, "vec", Arc::clone(&moded_spec));
        math_pkg.set_visible_modes(Some(vec![Mode::Math]));
        let mut base: Package<ModedLang> = Package::new("allmodes");
        base.insert(MACRO, "vec", Arc::clone(&text_spec));
        let mut stack: ScopeStack<ModedLang> = ScopeStack::new();
        stack.push(Arc::new(base));
        stack.push(Arc::new(math_pkg));
        let in_text = stack.iter_symbols(MACRO, Mode::Text);
        assert!(Arc::ptr_eq(in_text[0].spec, &text_spec));
        let in_math = stack.iter_symbols(MACRO, Mode::Math);
        assert!(Arc::ptr_eq(in_math[0].spec, &moded_spec));
    }

    #[test]
    fn escape_shadowing_check_reports_per_provider_and_type() {
        use crate::latexlike::{
            CallableType, EnvironmentSpec, Latexlike, MacroSpec,
        };
        use crate::source::Source;

        // Identifier stability: the reserved wire identifier, frozen by the slate.
        assert_eq!(
            ProviderCommandsShadowedByEscape::IDENTIFIER,
            "core.specs.provider-commands-shadowed-by-escape"
        );

        // One provider, two tables: the Environment table is all-escape-shadowed,
        // the Macro table is clean — exactly one warning, naming the shadowed
        // type. (Pooling the tables would let the clean entry silence it.)
        let mut package: Package<Latexlike> = Package::new("mixed");
        package.insert(CallableType::Macro, "good", Arc::new(MacroSpec::default()));
        package.insert(
            CallableType::Environment,
            r"\align",
            Arc::new(EnvironmentSpec::new(alloc::vec![])),
        );
        let state =
            crate::state::ParsingState::<Latexlike>::lang_initial_with_packages([package])
                .expect("seed state");
        let source = Arc::new(Source::new("any content"));
        let mut diagnostics = crate::error::Diagnostics::new();
        check_provider_commands_shadowed_by_escape(&state, &source, &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
        let diagnostic = diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.severity(), crate::error::Severity::Warning);
        let condition = diagnostic
            .data()
            .downcast_ref::<ProviderCommandsShadowedByEscape>()
            .unwrap();
        assert_eq!(condition.provider, "mixed");
        assert_eq!(condition.callable_type, "Environment");
        assert_eq!(condition.count, 1);
        assert_eq!(condition.example, r"\align");
        assert_eq!(condition.escape_chars, "\\");
    }

    #[test]
    fn closed_vocabulary_drives_whole_scope_enumeration() {
        use crate::state::ClosedVocabulary;

        // The tooling pattern the trait exists for: enumerate every invocation form of
        // a language whose vocabulary is statically listable.
        fn all_symbols<L: Lang>(stack: &ScopeStack<L>, mode: L::ModeId) -> Vec<(L::CallableTypeId, Box<str>)>
        where
            L::CallableTypeId: ClosedVocabulary,
        {
            let mut all = Vec::new();
            for &callable_type in L::CallableTypeId::ALL {
                for entry in stack.iter_symbols(callable_type, mode) {
                    all.push((entry.callable_type, Box::from(entry.name)));
                }
            }
            all
        }

        use crate::latexlike::{CallableType, Latexlike, Mode as LMode};
        let seed = crate::state::ParsingState::<Latexlike>::lang_initial_with_packages([
            crate::latexlike::minidefs::minilatex_package(),
        ]).expect("seed state");
        let symbols = all_symbols(&seed.scopes().clone(), LMode::Text);
        let has = |ct: CallableType, name: &str| {
            symbols.iter().any(|(t, n)| *t == ct && &**n == name)
        };
        // The builtin package's \begin/\end are ordinary Macro entries; minilatex's
        // typography ligatures are Specials rows named by their trigger.
        assert!(has(CallableType::Macro, "begin"));
        assert!(has(CallableType::Macro, "end"));
        assert!(has(CallableType::Specials, "--"));
        assert!(has(CallableType::Specials, "~"));
        // The ligatures are text-only: invisible under Math.
        let in_math = all_symbols(&seed.scopes().clone(), LMode::Math);
        assert!(!in_math.iter().any(|(t, n)| *t == CallableType::Specials && &**n == "--"));
        // ALL lists the full vocabularies.
        assert_eq!(CallableType::ALL.len(), 3);
        assert_eq!(<() as ClosedVocabulary>::ALL, &[()]);
    }
}
