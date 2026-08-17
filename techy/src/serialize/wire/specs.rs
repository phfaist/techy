//! The serialized shapes of the crate's own callable specs and providers: a package
//! by identity ([`WirePackage`]), a scope and a fallback provider in full
//! ([`WireScope`], [`WireFallbackProvider`] — their specs as positions in the specs
//! table), a spec by identity ([`WireSpecIdentity`] — its provider's position plus the
//! key it is defined under), and the error spec's self-contained form
//! ([`WireErrorSpec`]). Written and read by the impls in
//! [`drivers::specs`](crate::serialize::drivers::specs). The callable types are
//! carried as [`SerialValue`]s produced by the language's own conversion.

use alloc::string::String;
use alloc::vec::Vec;

use crate::serialize::drivers::{ProviderIndex, SpecIndex};
use crate::serialize::value::SerialValue;

use super::{FromSerialValue, ToSerialValue};

/// A package, by identity: its name. Provisional wire names (the vocabulary of the
/// serialized form is finalized before the schema is frozen).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WirePackage {
    /// The package's name.
    #[serial(name = "name")]
    pub(crate) name: String,
}

/// A scope, in full: its name and its definitions.
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireScope {
    /// The scope's name.
    #[serial(name = "name")]
    pub(crate) name: String,
    /// The definitions, ordered by callable type then name.
    #[serial(name = "definitions")]
    pub(crate) definitions: Vec<WireDefinition>,
}

/// One definition of a scope.
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireDefinition {
    /// The invocation form, in the language's own form (`CallableTypeId`'s value
    /// conversion).
    #[serial(name = "callable_type")]
    pub(crate) callable_type: SerialValue,
    /// The defined name.
    #[serial(name = "name")]
    pub(crate) name: String,
    /// The spec.
    #[serial(name = "spec")]
    pub(crate) spec: SpecIndex,
}

/// A fallback provider, in full: its name and its per-callable-type fallbacks.
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireFallbackProvider {
    /// The provider's name.
    #[serial(name = "name")]
    pub(crate) name: String,
    /// The fallbacks, ordered by callable type.
    #[serial(name = "fallbacks")]
    pub(crate) fallbacks: Vec<WireFallback>,
}

/// One fallback of a fallback provider.
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireFallback {
    /// The invocation form, in the language's own form.
    #[serial(name = "callable_type")]
    pub(crate) callable_type: SerialValue,
    /// The fallback spec.
    #[serial(name = "spec")]
    pub(crate) spec: SpecIndex,
}

/// A spec by identity: the provider that defines it, the invocation form, and the
/// key it is defined under.
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireSpecIdentity {
    /// The defining provider.
    #[serial(name = "provider")]
    pub(crate) provider: ProviderIndex,
    /// The invocation form, in the language's own form.
    #[serial(name = "callable_type")]
    pub(crate) callable_type: SerialValue,
    /// The key.
    #[serial(name = "key")]
    pub(crate) key: WireDefinitionKey,
}

/// The key a spec is defined under: a callable name or a specials trigger.
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) enum WireDefinitionKey {
    /// A callable name.
    #[serial(name = "name")]
    Name(String),
    /// A specials trigger.
    #[serial(name = "trigger")]
    Trigger(String),
}

/// The error spec's self-contained form.
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireErrorSpec {
    /// The optional detail of the diagnostic.
    #[serial(name = "detail")]
    pub(crate) detail: Option<String>,
}
