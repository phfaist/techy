//! The capability traits: [`SerializableObject`] (write side), [`DeserializableObject`]
//! (read side), and the [`SerializableLang`] declaration that makes both usable for a
//! language.

use crate::state::Lang;

use super::engine::{DeserializeContext, SerializeContext};
use super::error::{DeserializeError, SerializeError};
use super::value::{SerialEntry, SerialValue};

/// A language that supports serialization. Implementing this marker for a
/// [`Lang`] is what makes serialization and deserialization available for that
/// language: a [`SerializeContext`] or [`DeserializeContext`] can only exist for a
/// `SerializableLang`, so the serialization methods on the traits below — bounded
/// `where L: SerializableLang` — can be called exactly for such languages.
///
/// The trait has no items yet; the language-specific conversions it will supply are
/// not part of the crate at this stage.
pub trait SerializableLang: Lang {}

/// The write side of the serialization capability: an object that can produce its
/// serialized form.
///
/// Every [`CallableSpec`](crate::spec::CallableSpec) and every
/// [`SpecsProvider`](crate::scopes::SpecsProvider) implements this trait — it is a
/// supertrait of both, so that the method is callable through their trait objects
/// (`dyn CallableSpec<L>`, `dyn SpecsProvider<L>`), where the concrete type is
/// unknown. The method is defaulted to "unsupported", so a type that does not
/// participate in serialization writes exactly one line:
///
/// ```
/// use techy::core::specs::CallableSpec;
/// use techy::latexlike::Latexlike;
/// use techy::serialize::SerializableObject;
///
/// #[derive(Debug)]
/// struct MySpec;
///
/// // `MySpec` does not participate in serialization: the empty impl says so.
/// impl SerializableObject<Latexlike> for MySpec {}
///
/// impl CallableSpec<Latexlike> for MySpec {}
/// ```
///
/// A participating type overrides [`serialize_object`](Self::serialize_object). The
/// method is available only when the language is a [`SerializableLang`]: for any
/// other language it cannot be called (and no context value exists to call it with).
pub trait SerializableObject<L: Lang> {
    /// Produce this object's serialized form: its identifier and its data as a
    /// [`SerialEntry`]. `cx` gives the call access to the state of the serialization
    /// in progress.
    ///
    /// # Errors
    ///
    /// The default reports [`SerializeError::Unsupported`]: the type does not
    /// participate in serialization. An implementation returns an error when it
    /// cannot produce its serialized form.
    fn serialize_object(
        &self,
        cx: &mut SerializeContext<'_, L>,
    ) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        let _ = cx;
        Err(SerializeError::unsupported())
    }
}

/// The read side of the serialization capability: a type that can rebuild an object
/// from its serialized data. Opt-in and implemented by concrete types only: it is
/// never a supertrait (its associated type and its constructor — a function without a
/// `self` argument — would make the spec/provider traits unusable as trait objects),
/// and a type that does not participate implements nothing.
///
/// [`Output`](Self::Output) is what the read produces: the type itself for a type
/// rebuilt from a self-contained description, or a shared handle to an already
/// existing object (`Arc<dyn …>`) for a type that is looked up in the *reading
/// environment* — the live objects the deserializing program already holds (see the
/// [module documentation](crate::serialize)) — rather than rebuilt.
pub trait DeserializableObject<L: SerializableLang>: Sized {
    /// What [`deserialize_object`](Self::deserialize_object) produces.
    type Output;

    /// Rebuild an object from its serialized `value`. `cx` gives the call access to
    /// the state of the deserialization in progress.
    ///
    /// # Errors
    ///
    /// `value` is untrusted input: a value of the wrong shape, an index out of range,
    /// or a reference to an object the reading environment (the live objects the
    /// deserializing program already holds) lacks is an error, never a panic.
    fn deserialize_object(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self::Output, DeserializeError>;
}
