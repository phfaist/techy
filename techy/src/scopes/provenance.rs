//! [`SpecProvenance`]: the record, carried by a callable spec, of which provider
//! defined it under which key — what lets a spec that lives in a shared
//! [`Package`](super::Package) be serialized by *identity* (a reference to its
//! provider and its key) rather than described in full; and [`DefinitionKey`], the
//! key a provider defines a spec under.

use alloc::boxed::Box;
use alloc::sync::{Arc, Weak};
use core::fmt;

use crate::state::Lang;

use super::SpecsProvider;

/// The key a provider defines a spec under: a callable **name** (the normalized
/// spelling [`Package::insert`](super::Package::insert) registers, `"emph"` for
/// `\emph`) or a specials **trigger** (the character sequence
/// [`Package::insert_specials`](super::Package::insert_specials) registers, `"---"`).
/// The two are separate keys — a package keeps its named definitions and its specials
/// in separate stores, and a name and a trigger with the same spelling do not
/// collide — so a [`SpecProvenance`] records which of the two it is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DefinitionKey {
    /// A callable name, as registered (without the escape character).
    Name(Box<str>),
    /// A specials trigger sequence, as registered.
    Trigger(Box<str>),
}

impl DefinitionKey {
    /// The key's spelling: the name or the trigger.
    pub fn as_str(&self) -> &str {
        match self {
            DefinitionKey::Name(key) | DefinitionKey::Trigger(key) => key,
        }
    }
}

impl fmt::Display for DefinitionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DefinitionKey::Name(name) => write!(f, "name `{name}`"),
            DefinitionKey::Trigger(trigger) => write!(f, "trigger `{trigger}`"),
        }
    }
}

/// Where a callable spec was defined: the provider that holds it (a weak reference —
/// the provider holds the spec strongly, and a strong reference back would keep both
/// alive forever), the invocation form ([`Lang::CallableTypeId`]) and the
/// [`DefinitionKey`] it was defined under. A *provenance stamp*.
///
/// The stamp is what makes a spec serializable by identity: the spec's serialized
/// form is then a reference to its provider's entry plus the key, and the reading
/// side looks the spec up in the corresponding provider of its own environment
/// ([`KnownProviders`](crate::serialize::KnownProviders)) — the very instance that
/// provider holds. The crate's own [`Package`](super::Package) hands out stamps for
/// its definitions when it is built shared ([`Package::new_shared`](super::Package::new_shared),
/// [`Package::provenance_for`](super::Package::provenance_for)); a concrete spec type
/// carries the stamp in a field set at construction (`with_provenance` on the
/// crate's spec types) and its
/// [`serialize_object`](crate::serialize::SerializableObject::serialize_object) writes
/// the identity form through the stamp's own
/// [`SerializableObject`](crate::serialize::SerializableObject) impl. A stamp is
/// process-local: it never appears in serialized data itself.
///
/// The reading side of the crate resolves stamped specs whose provider is a
/// [`Package`](super::Package) — the identity entry names the provider by position
/// and the reader looks the key up in that package (`get` for a name, `get_specials`
/// for a trigger). A custom provider type that stamps its specs registers its own
/// reader for its own spec entries; the stamp type is reusable for that (its fields
/// are public through the accessors below), the crate's reader is not.
pub struct SpecProvenance<L: Lang> {
    provider: Weak<dyn SpecsProvider<L>>,
    callable_type: L::CallableTypeId,
    key: DefinitionKey,
}

impl<L: Lang> SpecProvenance<L> {
    /// A stamp naming `provider` (weakly), the invocation form, and the key.
    pub fn new(
        provider: Weak<dyn SpecsProvider<L>>,
        callable_type: L::CallableTypeId,
        key: DefinitionKey,
    ) -> SpecProvenance<L> {
        SpecProvenance { provider, callable_type, key }
    }

    /// The defining provider, if it is still alive.
    pub fn provider(&self) -> Option<Arc<dyn SpecsProvider<L>>> {
        self.provider.upgrade()
    }

    /// The invocation form the spec was defined under.
    pub fn callable_type(&self) -> L::CallableTypeId {
        self.callable_type
    }

    /// The key the spec was defined under.
    pub fn key(&self) -> &DefinitionKey {
        &self.key
    }
}

impl<L: Lang> Clone for SpecProvenance<L> {
    fn clone(&self) -> Self {
        SpecProvenance {
            provider: Weak::clone(&self.provider),
            callable_type: self.callable_type,
            key: self.key.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for SpecProvenance<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let provider = self.provider.upgrade();
        f.debug_struct("SpecProvenance")
            .field("provider", &provider.as_ref().map(|provider| provider.name()))
            .field("callable_type", &self.callable_type)
            .field("key", &self.key)
            .finish()
    }
}
