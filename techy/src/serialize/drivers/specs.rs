//! The serialization of the crate's own callable specs and providers — the
//! [`SerializableObject`] / [`DeserializableObject`] impls of [`Package`] (by
//! identity: its name), [`Scope`] and [`FallbackProvider`] (in full: their
//! definitions as spec positions), [`ErrorCallableSpec`] (its self-contained form),
//! [`SpecProvenance`] (the identity form of a stamped spec: provider position plus
//! key) and [`StdCallableSpec`] (identity through its stamp) — the reading
//! environment's provider directory [`KnownProviders`] with its [`ProviderRecipe`]s,
//! and [`register_core_readers`], which registers the readers of all of the above.
//!
//! **Identity and self-contained forms.** A provider or spec is serialized either by
//! *identity* — a reference the reading side resolves against the live objects it
//! already holds — or in a *self-contained* form the reading side rebuilds an
//! equivalent object from. A package is part of the reading program's own
//! configuration: identity by name. A package's
//! specs hold argument parsers, which have no serialized form: identity through the
//! provenance stamp the (shared) package handed out — a reference to the package's
//! own entry plus the definition key, resolved by looking the key up in the reading
//! side's package of that name. Scopes hold definitions made during the parse:
//! written in full, each definition's spec as a position in the specs table (where
//! the spec is whatever it is — an identity or a self-contained entry). Fallback
//! providers likewise. The error spec is plain data. Every read validates its input
//! and names what is missing ([`DeserializeError::MissingProvider`],
//! [`DeserializeError::MissingDefinition`]).

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt;

use crate::scopes::{
    DefinitionKey, ErrorCallableSpec, FallbackProvider, IntoSpecsProvider, Package, Scope,
    SpecProvenance, SpecsProvider,
};
use crate::spec::{CallableSpec, StdCallableSpec};
use crate::state::Lang;

use super::super::engine::{DeserializeContext, SerdeSession, SerializeContext};
use super::super::error::{DeserializeError, RegistrationError, SerializeError};
use super::super::object::{
    DeserializableObject, DeserializableValue, SerializableLang, SerializableObject,
    SerializableValue,
};
use super::super::value::{SerialEntry, SerialValue};
use super::super::wire::specs::{
    WireDefinition, WireDefinitionKey, WireErrorSpec, WireFallback, WireFallbackProvider,
    WirePackage, WireScope, WireSpecIdentity,
};
use super::super::wire::{FromSerialValue, ToSerialValue};
use super::standard::{StandardTableInterning, StandardTableReading};
use super::{
    ERROR_SPEC_IDENTIFIER, FALLBACK_PROVIDER_IDENTIFIER, PACKAGE_IDENTIFIER, SCOPE_IDENTIFIER,
    SPEC_IDENTITY_IDENTIFIER,
};

// --- the reading environment: KnownProviders ------------------------------------------

/// A way to build a provider the reading environment does not hold yet: what
/// [`KnownProviders::register_recipe`] takes. A recipe is built at most once per
/// serialized provider entry — the built provider is stored in the session's
/// providers table, and every reference to that entry yields it — but a second
/// session, or a second entry naming the same provider (a writer holding two distinct
/// providers of one name), builds it again.
///
/// Any `Fn() -> P` closure or function whose `P` converts into a shared provider
/// ([`IntoSpecsProvider`]) is a recipe: `known.register_recipe("minilatex",
/// minilatex_package::<Latexlike>)`. A recipe that can fail (loading a package from a
/// file, say) implements the trait itself and reports through `build`'s `Result`.
pub trait ProviderRecipe<L: Lang>: Send + Sync {
    /// Build the provider.
    ///
    /// # Errors
    ///
    /// The provider cannot be built (typically [`DeserializeError::failed`] with the
    /// reason).
    fn build(&self) -> Result<Arc<dyn SpecsProvider<L>>, DeserializeError>;
}

impl<L: Lang, F, P> ProviderRecipe<L> for F
where
    F: Fn() -> P + Send + Sync,
    P: IntoSpecsProvider<L>,
{
    fn build(&self) -> Result<Arc<dyn SpecsProvider<L>>, DeserializeError> {
        Ok(self().into_specs_provider())
    }
}

/// The providers of the reading environment, by name: what a serialized
/// [`Package`] entry (identity by name) resolves against — a provider directory the
/// deserializing program fills with the providers it already holds
/// ([`insert`](KnownProviders::insert)) and, for providers it does not hold but
/// knows how to build, with [`ProviderRecipe`]s
/// ([`register_recipe`](KnownProviders::register_recipe)). Set on the session as
/// its user data ([`SerdeSession::set_user_data`]); the package reader looks it up
/// through the context.
///
/// Resolution ([`resolve`](KnownProviders::resolve)): the provider inserted under
/// the name wins; else the recipe registered under the name is built (once per
/// serialized entry — the session keeps the built provider in its table); else the
/// name is unknown ([`DeserializeError::MissingProvider`] at the reading site).
/// Names need not be unique across a parse's providers in general
/// ([`SpecsProvider::name`]); here they are the keys — the deserializing program
/// decides which provider each name means in its environment.
///
/// A language's `serialize::register` helper typically fills in the recipes of the
/// language's own packages (for the latexlike preset:
/// [`latexlike::serialize::register_package_recipes`](crate::latexlike::serialize::register_package_recipes)).
///
/// ```
/// use techy::core::specs::Package;
/// use techy::core::TrivialLang;
/// use techy::serialize::{KnownProviders, SerdeSession, SerializableLang};
///
/// #[derive(Debug, Clone, Copy)]
/// struct MyLang;
/// impl TrivialLang for MyLang {}
/// impl SerializableLang for MyLang {}
///
/// let mut known = KnownProviders::<MyLang>::new();
/// // A provider the program holds: identity.
/// known.insert(Package::<MyLang>::new_shared("mydefs", |_package| {}));
/// // A provider it can build on demand: a recipe (here, a function item).
/// fn base() -> Package<MyLang> { Package::new("base") }
/// known.register_recipe("base", base);
///
/// assert!(known.get("mydefs").is_some());
/// assert!(known.get("base").is_none());   // not held — but resolvable
/// assert_eq!(known.resolve("base").unwrap().map(|p| p.name().to_string()), Some("base".to_string()));
///
/// let mut session = SerdeSession::<MyLang>::new();
/// session.set_user_data(known);
/// ```
pub struct KnownProviders<L: Lang> {
    providers: BTreeMap<String, Arc<dyn SpecsProvider<L>>>,
    recipes: BTreeMap<String, Arc<dyn ProviderRecipe<L>>>,
}

impl<L: Lang> KnownProviders<L> {
    /// An empty directory.
    pub fn new() -> KnownProviders<L> {
        KnownProviders { providers: BTreeMap::new(), recipes: BTreeMap::new() }
    }

    /// Hold `provider` under its own name ([`SpecsProvider::name`]) — a package by
    /// value, an `Arc<Package<L>>`, or any shared provider ([`IntoSpecsProvider`]).
    /// Returns the provider previously held under that name, if any (replaced).
    pub fn insert(&mut self, provider: impl IntoSpecsProvider<L>) -> Option<Arc<dyn SpecsProvider<L>>> {
        let provider = provider.into_specs_provider();
        self.providers.insert(String::from(provider.name()), provider)
    }

    /// The provider held under `name`, if any (recipes are not consulted — see
    /// [`resolve`](KnownProviders::resolve)).
    pub fn get(&self, name: &str) -> Option<&Arc<dyn SpecsProvider<L>>> {
        self.providers.get(name)
    }

    /// Register `recipe` as the way to build the provider named `name` when no
    /// provider is held under that name. The provider the recipe builds must answer
    /// `name` as its own [`name()`](crate::scopes::SpecsProvider::name):
    /// [`resolve`](KnownProviders::resolve) checks it and reports a recipe that builds
    /// a provider of another name as an error. Returns the recipe previously
    /// registered under `name`, if any (replaced).
    pub fn register_recipe(
        &mut self,
        name: impl Into<String>,
        recipe: impl ProviderRecipe<L> + 'static,
    ) -> Option<Arc<dyn ProviderRecipe<L>>> {
        self.recipes.insert(name.into(), Arc::new(recipe))
    }

    /// The recipe registered under `name`, if any.
    pub fn recipe(&self, name: &str) -> Option<&Arc<dyn ProviderRecipe<L>>> {
        self.recipes.get(name)
    }

    /// The provider named `name`: the one held under that name, else the one the
    /// recipe registered under that name builds (built now — the caller keeps the
    /// result), else `None`.
    ///
    /// # Errors
    ///
    /// The recipe's own failure; the recipe built a provider whose
    /// [`name()`](crate::scopes::SpecsProvider::name) is not `name`
    /// ([`DeserializeError::Failed`], naming both).
    pub fn resolve(&self, name: &str) -> Result<Option<Arc<dyn SpecsProvider<L>>>, DeserializeError> {
        if let Some(provider) = self.providers.get(name) {
            return Ok(Some(Arc::clone(provider)));
        }
        match self.recipes.get(name) {
            Some(recipe) => {
                let provider = recipe.build()?;
                if provider.name() != name {
                    return Err(DeserializeError::failed(format!(
                        "the recipe registered for provider `{name}` built a provider named `{}`",
                        provider.name()
                    )));
                }
                Ok(Some(provider))
            }
            None => Ok(None),
        }
    }

    /// The names of the held providers, in order.
    pub fn provider_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.providers.keys().map(String::as_str)
    }

    /// The names of the registered recipes, in order.
    pub fn recipe_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.recipes.keys().map(String::as_str)
    }
}

impl<L: Lang> Default for KnownProviders<L> {
    fn default() -> Self {
        KnownProviders::new()
    }
}

impl<L: Lang> fmt::Debug for KnownProviders<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KnownProviders")
            .field("providers", &self.providers.keys().collect::<Vec<_>>())
            .field("recipes", &self.recipes.keys().collect::<Vec<_>>())
            .finish()
    }
}

// --- registration ---------------------------------------------------------------------

/// Register the readers of the crate's own spec and provider types on `session`'s
/// specs and providers tables (the standard tables, found by name): the identity
/// form of stamped specs ([`SpecProvenance`]), the error spec, packages (by
/// name, resolved through [`KnownProviders`]), scopes, and fallback providers. Call
/// it once per reading session — a language's own `serialize::register` helper does
/// (the latexlike preset's [`latexlike::serialize::register`](crate::latexlike::serialize::register)),
/// so a session prepared with such a helper needs no separate call.
///
/// # Errors
///
/// [`RegistrationError::UnknownTableName`] when the session lacks a standard table
/// (a session composed with [`SerdeSession::empty`] that did not register them
/// all); [`RegistrationError::DuplicateIdentifier`] when the readers are already
/// registered (the function was called twice, or a language helper already called
/// it).
pub fn register_core_readers<L: SerializableLang>(session: &mut SerdeSession<L>) -> Result<(), RegistrationError> {
    let tables = session
        .standard_tables()
        .ok_or_else(|| RegistrationError::UnknownTableName { name: String::from(super::missing_standard_table(session)) })?;
    tables.specs.register_type::<SpecProvenance<L>>(session, SPEC_IDENTITY_IDENTIFIER, |spec| spec)?;
    tables.specs.register_type::<ErrorCallableSpec>(session, ERROR_SPEC_IDENTIFIER, |spec| {
        Arc::new(spec) as Arc<dyn CallableSpec<L>>
    })?;
    tables.providers.register_type::<Package<L>>(session, PACKAGE_IDENTIFIER, |provider| provider)?;
    tables.providers.register_type::<Scope<L>>(session, SCOPE_IDENTIFIER, |scope| {
        Arc::new(scope) as Arc<dyn SpecsProvider<L>>
    })?;
    tables.providers.register_type::<FallbackProvider<L>>(session, FALLBACK_PROVIDER_IDENTIFIER, |provider| {
        Arc::new(provider) as Arc<dyn SpecsProvider<L>>
    })?;
    Ok(())
}

// --- SpecProvenance: the identity form of a stamped spec ------------------------------

/// The identity form of a stamped spec (identifier `core.provider-spec-identity`):
/// `{provider, callable_type, definition}` — the
/// defining provider's position in the providers table (interned here: the provider
/// is written once, wherever it is referred to from), the invocation form in the
/// language's own form, and the definition key (`{name: …}` or `{trigger: …}`). This
/// is what a stamped spec's own `serialize_object` delegates to.
impl<L: Lang> SerializableObject<L> for SpecProvenance<L> {
    /// # Errors
    ///
    /// [`SerializeError::ProviderDropped`] when the defining provider no longer
    /// exists; the errors of interning the provider.
    fn serialize_object(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        let provider = self.provider().ok_or_else(|| SerializeError::ProviderDropped {
            callable_type: format!("{:?}", self.callable_type()),
            key: self.key().clone(),
        })?;
        let wire = WireSpecIdentity {
            provider: cx.intern_provider(&provider)?,
            callable_type: self.callable_type().serialize_value(cx)?,
            key: match self.key() {
                DefinitionKey::Name(name) => WireDefinitionKey::Name(name.to_string()),
                DefinitionKey::Trigger(trigger) => WireDefinitionKey::Trigger(trigger.to_string()),
            },
        };
        Ok(SerialEntry { identifier: SPEC_IDENTITY_IDENTIFIER.into(), data: wire.to_serial_value()? })
    }
}

/// Resolves the identity form against the reading environment: the provider at the
/// entry's position (read through the providers table — a package the environment
/// supplied, see [`KnownProviders`]) must be a [`Package`], and the spec is the one
/// it holds under the key ([`Package::get`] for a name, [`Package::get_specials`] for
/// a trigger) — the very instance, so specs shared between nodes and the package stay
/// shared. A provider of another type is not resolved here: a custom provider type
/// that stamps its specs registers its own reader for its own spec entries.
impl<L: SerializableLang> DeserializableObject<L> for SpecProvenance<L> {
    type Output = Arc<dyn CallableSpec<L>>;

    /// # Errors
    ///
    /// The value's shape; the provider position's errors; [`DeserializeError::Failed`]
    /// when the provider is not a package; [`DeserializeError::MissingDefinition`]
    /// when the package holds nothing under the key.
    fn deserialize_object(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self::Output, DeserializeError> {
        let wire = WireSpecIdentity::from_serial_value(value)?;
        let provider = cx.provider(wire.provider)?;
        let callable_type = <L::CallableTypeId as DeserializableValue<L>>::deserialize_value(&wire.callable_type, cx)?;
        let key = match wire.key {
            WireDefinitionKey::Name(name) => DefinitionKey::Name(name.into_boxed_str()),
            WireDefinitionKey::Trigger(trigger) => DefinitionKey::Trigger(trigger.into_boxed_str()),
        };
        let Some(package) = (&*provider as &dyn Any).downcast_ref::<Package<L>>() else {
            return Err(DeserializeError::failed(format!(
                "the spec's identity names the provider `{}`, which is not a Package: the \
                 crate resolves spec identities in packages only (a custom provider type \
                 registers its own reader for its specs' entries)",
                provider.name()
            )));
        };
        let spec = match &key {
            DefinitionKey::Name(name) => package.get(callable_type, name),
            DefinitionKey::Trigger(trigger) => package.get_specials(callable_type, trigger),
        };
        spec.cloned().ok_or_else(|| DeserializeError::MissingDefinition {
            provider: String::from(package.name()),
            callable_type: format!("{callable_type:?}"),
            key,
        })
    }
}

/// The identity form of a stamped spec, or the error naming the spec's type when the
/// spec carries no stamp — the shared body of the crate's identity-only spec types'
/// `serialize_object`.
pub(crate) fn serialize_stamped_spec<L: SerializableLang>(
    provenance: Option<&SpecProvenance<L>>,
    spec_type: &'static str,
    cx: &mut SerializeContext<'_, L>,
) -> Result<SerialEntry, SerializeError> {
    match provenance {
        Some(provenance) => provenance.serialize_object(cx),
        None => Err(SerializeError::MissingProvenance { spec: spec_type }),
    }
}

/// By identity through its provenance stamp
/// ([`with_provenance`](StdCallableSpec::with_provenance)); an unstamped spec is
/// [`SerializeError::MissingProvenance`].
impl<L: Lang> SerializableObject<L> for StdCallableSpec<L> {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        serialize_stamped_spec(self.provenance.as_ref(), "StdCallableSpec", cx)
    }
}

// --- ErrorCallableSpec: self-contained ---------------------------------------------------

/// Self-contained: `{detail?}`.
impl<L: Lang> SerializableObject<L> for ErrorCallableSpec {
    fn serialize_object(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        let wire = WireErrorSpec { detail: self.detail.as_deref().map(String::from) };
        Ok(SerialEntry { identifier: ERROR_SPEC_IDENTIFIER.into(), data: wire.to_serial_value()? })
    }
}

impl<L: SerializableLang> DeserializableObject<L> for ErrorCallableSpec {
    type Output = ErrorCallableSpec;

    fn deserialize_object(
        value: &SerialValue,
        _cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self::Output, DeserializeError> {
        let wire = WireErrorSpec::from_serial_value(value)?;
        Ok(ErrorCallableSpec { detail: wire.detail.map(String::into_boxed_str) })
    }
}

// --- Package: identity by name ---------------------------------------------------------

/// By identity: `{name}`.
impl<L: Lang> SerializableObject<L> for Package<L> {
    fn serialize_object(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        let wire = WirePackage { name: String::from(self.name()) };
        Ok(SerialEntry { identifier: PACKAGE_IDENTIFIER.into(), data: wire.to_serial_value()? })
    }
}

/// Resolved against the reading environment: the [`KnownProviders`] in the session's
/// user data — the provider held under the name, else the one its recipe builds
/// (kept by the session for every reference to this entry).
impl<L: SerializableLang> DeserializableObject<L> for Package<L> {
    type Output = Arc<dyn SpecsProvider<L>>;

    /// # Errors
    ///
    /// The value's shape; [`DeserializeError::MissingProvider`] when no
    /// `KnownProviders` is set or it knows neither a provider nor a recipe of the
    /// name; the recipe's own failure.
    fn deserialize_object(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self::Output, DeserializeError> {
        let wire = WirePackage::from_serial_value(value)?;
        let missing = || DeserializeError::MissingProvider { name: wire.name.clone() };
        let known = cx.user_data::<KnownProviders<L>>().ok_or_else(missing)?;
        known.resolve(&wire.name)?.ok_or_else(missing)
    }
}

// --- Scope: in full ---------------------------------------------------------------------

/// In full: `{name, definitions: [{callable_type, name, spec}]}`, the definitions
/// ordered by callable type then name, each spec interned into the specs table (an
/// identity or a self-contained entry, whatever the spec's type writes).
impl<L: Lang> SerializableObject<L> for Scope<L> {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        let mut definitions = Vec::with_capacity(self.len());
        for (callable_type, name, spec) in self.definitions() {
            definitions.push(WireDefinition {
                callable_type: callable_type.serialize_value(cx)?,
                name: String::from(name),
                spec: cx.intern_spec(spec)?,
            });
        }
        let wire = WireScope { name: String::from(self.name()), definitions };
        Ok(SerialEntry { identifier: SCOPE_IDENTIFIER.into(), data: wire.to_serial_value()? })
    }
}

/// Rebuilt from its definitions ([`Scope::new`] + [`Scope::insert`]), each spec read
/// from the specs table.
impl<L: SerializableLang> DeserializableObject<L> for Scope<L> {
    type Output = Scope<L>;

    /// # Errors
    ///
    /// The value's shape; a definition listed twice ([`DeserializeError::Failed`]);
    /// the errors of reading a spec.
    fn deserialize_object(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self::Output, DeserializeError> {
        let wire = WireScope::from_serial_value(value)?;
        let mut scope = Scope::new(wire.name);
        for definition in wire.definitions {
            let callable_type =
                <L::CallableTypeId as DeserializableValue<L>>::deserialize_value(&definition.callable_type, cx)?;
            let spec = cx.spec(definition.spec)?;
            if scope.insert(callable_type, definition.name.as_str(), spec).is_some() {
                return Err(DeserializeError::failed(format!(
                    "the scope `{}` lists the definition of `{}` under {callable_type:?} twice",
                    scope.name(),
                    definition.name
                )));
            }
        }
        Ok(scope)
    }
}

// --- FallbackProvider: in full ---------------------------------------------------------

/// In full: `{name, fallbacks: [{callable_type, spec}]}`, ordered by callable type,
/// each spec interned into the specs table.
impl<L: Lang> SerializableObject<L> for FallbackProvider<L> {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        let mut fallbacks = Vec::new();
        for (callable_type, spec) in self.fallbacks() {
            fallbacks.push(WireFallback {
                callable_type: callable_type.serialize_value(cx)?,
                spec: cx.intern_spec(spec)?,
            });
        }
        let wire = WireFallbackProvider { name: String::from(self.name()), fallbacks };
        Ok(SerialEntry { identifier: FALLBACK_PROVIDER_IDENTIFIER.into(), data: wire.to_serial_value()? })
    }
}

/// Rebuilt from its fallbacks ([`FallbackProvider::new`] + [`FallbackProvider::set`]).
impl<L: SerializableLang> DeserializableObject<L> for FallbackProvider<L> {
    type Output = FallbackProvider<L>;

    /// # Errors
    ///
    /// The value's shape; a callable type listed twice ([`DeserializeError::Failed`]);
    /// the errors of reading a spec.
    fn deserialize_object(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self::Output, DeserializeError> {
        let wire = WireFallbackProvider::from_serial_value(value)?;
        let mut provider = FallbackProvider::new(wire.name);
        for fallback in wire.fallbacks {
            let callable_type =
                <L::CallableTypeId as DeserializableValue<L>>::deserialize_value(&fallback.callable_type, cx)?;
            let spec = cx.spec(fallback.spec)?;
            if provider.set(callable_type, spec).is_some() {
                return Err(DeserializeError::failed(format!(
                    "the fallback provider `{}` lists a fallback for {callable_type:?} twice",
                    provider.name()
                )));
            }
        }
        Ok(provider)
    }
}

/// The unit spec types' empty self-contained form (`{}`), for the crate's own unit
/// specs: no fields, so nothing to write; reading accepts an empty map only.
pub(crate) fn unit_recipe_value() -> SerialValue {
    SerialValue::Map(Vec::new())
}

/// Read the empty self-contained form of a unit spec: an empty map (any key is an
/// error, as is any other kind of value).
pub(crate) fn read_unit_recipe(value: &SerialValue, what: &'static str) -> Result<(), DeserializeError> {
    super::super::wire::FieldReader::new(value, what, &[])?;
    Ok(())
}
