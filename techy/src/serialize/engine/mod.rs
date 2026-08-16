//! The serialization engine: the contexts handed to serialization and
//! deserialization calls. At this stage the contexts are shells with no operations;
//! the session machinery that will stand behind them is not yet present.

use core::marker::PhantomData;

use super::object::SerializableLang;

/// The serialization call's access to the state of the serialization in progress —
/// the `cx` argument of
/// [`serialize_object`](crate::serialize::SerializableObject::serialize_object) and
/// [`serialize_argument_spec`](crate::spec::CallableSpec::serialize_argument_spec).
///
/// A context exists only for a [`SerializableLang`]: that bound is what makes the
/// serialization methods, which take a context, callable exactly for languages that
/// support serialization. Constructed by the crate's serialization machinery only.
/// It has no public operations yet.
pub struct SerializeContext<'a, L: SerializableLang> {
    // Placeholder for the session state the context will borrow (`'a`) for `L`.
    _shell: PhantomData<&'a mut L>,
}

/// The deserialization call's access to the state of the deserialization in
/// progress — the `cx` argument of
/// [`deserialize_object`](crate::serialize::DeserializableObject::deserialize_object)
/// and
/// [`deserialize_argument_spec`](crate::spec::CallableSpec::deserialize_argument_spec).
///
/// A context exists only for a [`SerializableLang`]: that bound is what makes the
/// deserialization methods, which take a context, callable exactly for languages that
/// support serialization. Constructed by the crate's serialization machinery only.
/// It has no public operations yet.
pub struct DeserializeContext<'a, L: SerializableLang> {
    // Placeholder for the session state the context will borrow (`'a`) for `L`.
    _shell: PhantomData<&'a mut L>,
}

// Test-only constructors: the shells have no session behind them, so a unit test that
// needs to call a serialization method builds one this way. The real constructors
// arrive with the session machinery.
#[cfg(test)]
impl<L: SerializableLang> SerializeContext<'_, L> {
    pub(crate) fn shell() -> Self {
        SerializeContext { _shell: PhantomData }
    }
}

#[cfg(test)]
impl<L: SerializableLang> DeserializeContext<'_, L> {
    pub(crate) fn shell() -> Self {
        DeserializeContext { _shell: PhantomData }
    }
}
