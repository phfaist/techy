//! Heterogeneous tables: [`DispatchingSerdeDriver`], the driver of a table holding
//! trait objects of several concrete types; [`ObjectReader`], the registered way to
//! rebuild the objects of one identifier; [`IdentifierResolver`], a namespace's
//! supplier of readers; and the registration methods on the table's handle.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;

use hashbrown::HashMap;

use super::super::error::{DeserializeError, RegistrationError, SerializeError};
use super::super::object::{DeserializableObject, SerializableLang, SerializableObject};
use super::super::value::{SerialEntry, SerialIndex, SerialValue};
use super::context::{DeserializeContext, SerializeContext};
use super::driver::{ObjectSerdeDriver, TableHandle};
use super::session::SerdeSession;

/// The driver of a *heterogeneous* table: a table whose objects are trait objects
/// (`Arc<T>` with `T = dyn …`) of several concrete types, each with its own
/// identifier and serialized form.
///
/// Writing needs no registration: every object serializes itself through its own
/// [`SerializableObject::serialize_object`] (a supertrait method, so it is reached
/// through the trait object). Reading dispatches on the entry's identifier: the
/// readers registered on the table's handle
/// ([`TableHandle::register_type`] / [`register_reader`](TableHandle::register_reader)),
/// then the registered [`IdentifierResolver`]s
/// ([`register_resolver`](TableHandle::register_resolver)) — a resolver's answer is
/// kept, so it is asked once per identifier for the session's lifetime — and,
/// when nothing recognizes the identifier, the fail-closed
/// [`DeserializeError::UnknownIdentifier`]. Registrations belong to the session
/// (the driver holds none), so a driver value can be registered in any number of
/// sessions.
///
/// `T` is the trait object type; `I` the table's position type (a
/// [`serial_index!`](crate::serialize::serial_index) type). The table is
/// heterogeneous ([`homogeneous_identifier`](ObjectSerdeDriver::homogeneous_identifier)
/// is `None`): entries carry their identifier on the wire.
pub struct DispatchingSerdeDriver<L, T: ?Sized, I> {
    table_name: &'static str,
    types: PhantomData<fn() -> (L, Arc<T>, I)>,
}

impl<L, T: ?Sized, I> DispatchingSerdeDriver<L, T, I> {
    /// The driver of the heterogeneous table named `table_name` (a deliberately
    /// chosen, stable name — see [`ObjectSerdeDriver::table_name`]).
    pub fn new(table_name: &'static str) -> Self {
        DispatchingSerdeDriver { table_name, types: PhantomData }
    }
}

impl<L, T: ?Sized, I> fmt::Debug for DispatchingSerdeDriver<L, T, I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DispatchingSerdeDriver").field("table_name", &self.table_name).finish()
    }
}

impl<L, T, I> ObjectSerdeDriver<L> for DispatchingSerdeDriver<L, T, I>
where
    L: SerializableLang,
    T: ?Sized + SerializableObject<L> + Send + Sync + 'static,
    I: SerialIndex,
{
    type Object = T;
    type Index = I;

    fn table_name(&self) -> &'static str {
        self.table_name
    }

    fn homogeneous_identifier(&self) -> Option<&'static str> {
        None
    }

    fn serialize_object(
        &self,
        object: &Arc<T>,
        cx: &mut SerializeContext<'_, L>,
    ) -> Result<SerialEntry, SerializeError> {
        object.serialize_object(cx)
    }

    fn deserialize_object(
        &self,
        entry: &SerialEntry,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Arc<T>, DeserializeError> {
        let reader = find_reader::<L, T>(cx, &entry.identifier)?;
        reader.deserialize_object(&entry.data, cx)
    }
}

/// How the objects of one identifier are rebuilt: the read side's registered unit,
/// held by a heterogeneous table's dispatch (see [`DispatchingSerdeDriver`]) and
/// produced by [`IdentifierResolver`]s. Wraps one deserialization routine producing
/// `Arc<T>`, either a type's own [`DeserializableObject`] impl
/// ([`from_type`](ObjectReader::from_type)) or any routine ([`new`](ObjectReader::new)
/// — how a resolver wraps a definition it loaded dynamically). Cheap to clone (the
/// routine is shared).
pub struct ObjectReader<L: SerializableLang, T: ?Sized> {
    read: Arc<dyn Fn(&SerialValue, &mut DeserializeContext<'_, L>) -> Result<Arc<T>, DeserializeError> + Send + Sync>,
}

impl<L: SerializableLang, T: ?Sized + Send + Sync + 'static> ObjectReader<L, T> {
    /// A reader from a routine: called with the entry's data and the context, it
    /// rebuilds the object.
    pub fn new(
        read: impl Fn(&SerialValue, &mut DeserializeContext<'_, L>) -> Result<Arc<T>, DeserializeError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        ObjectReader { read: Arc::new(read) }
    }

    /// A reader from a type's own [`DeserializableObject`] impl: `C::deserialize_object`
    /// rebuilds the value, and `wrap` turns it into the table's `Arc<T>` — `Arc::new`
    /// (with the coercion to the trait object) for a type whose `Output` is itself,
    /// the identity for a type whose `Output` is already an `Arc<T>` (an object looked
    /// up in the reading environment).
    pub fn from_type<C: DeserializableObject<L>>(
        wrap: impl Fn(C::Output) -> Arc<T> + Send + Sync + 'static,
    ) -> Self {
        ObjectReader::new(move |value, cx| C::deserialize_object(value, cx).map(&wrap))
    }

    /// Rebuild an object from `value`.
    ///
    /// # Errors
    ///
    /// Whatever the routine reports.
    pub fn deserialize_object(
        &self,
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Arc<T>, DeserializeError> {
        (self.read)(value, cx)
    }
}

impl<L: SerializableLang, T: ?Sized> Clone for ObjectReader<L, T> {
    fn clone(&self) -> Self {
        ObjectReader { read: Arc::clone(&self.read) }
    }
}

impl<L: SerializableLang, T: ?Sized> fmt::Debug for ObjectReader<L, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObjectReader").finish_non_exhaustive()
    }
}

/// A namespace's supplier of readers for a heterogeneous table: asked for an
/// identifier no registered reader covers, it answers with the [`ObjectReader`] for
/// it (constructed on demand — from a definition it loads, under its own trust
/// policy) or declines. Registered with a prefix on the table's handle
/// ([`TableHandle::register_resolver`]); a resolver is asked only for identifiers
/// beginning with its prefix, and its answer for an identifier is kept by the session
/// and never asked again.
pub trait IdentifierResolver<L: SerializableLang, T: ?Sized>: Send + Sync {
    /// The reader for `identifier`, or `None` to decline. `cx` gives access to the
    /// session — the caller's user data in particular.
    ///
    /// # Errors
    ///
    /// The resolver recognizes the identifier but cannot supply a reader for it
    /// (typically [`DeserializeError::failed`] with the reason); the deserialization
    /// fails with that error.
    fn resolve(
        &self,
        identifier: &str,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Option<ObjectReader<L, T>>, DeserializeError>;
}

/// The read-side registrations of one heterogeneous table, kept by the session:
/// the readers by identifier (registered ones and kept resolver answers) and the
/// resolvers by prefix, in registration order.
pub(super) struct ReadDispatch<L: SerializableLang, T: ?Sized> {
    readers: HashMap<String, ObjectReader<L, T>>,
    resolvers: Vec<(String, Arc<dyn IdentifierResolver<L, T>>)>,
}

impl<L: SerializableLang, T: ?Sized> Default for ReadDispatch<L, T> {
    fn default() -> Self {
        ReadDispatch { readers: HashMap::new(), resolvers: Vec::new() }
    }
}

/// The reader for `identifier` in the table of the entry `cx` is deserializing:
/// registered reader, else the resolvers whose prefix matches — the longest prefix
/// first, registration order among equal lengths — the first answer kept; else
/// [`DeserializeError::UnknownIdentifier`].
fn find_reader<L: SerializableLang, T: ?Sized + Send + Sync + 'static>(
    cx: &mut DeserializeContext<'_, L>,
    identifier: &str,
) -> Result<ObjectReader<L, T>, DeserializeError> {
    let unknown = |session: &SerdeSession<L>, ordinal: Option<usize>| DeserializeError::UnknownIdentifier {
        table: ordinal.map_or("?", |ordinal| session.table_name(ordinal)),
        identifier: String::from(identifier),
    };
    let Some((ordinal, _)) = cx.current() else {
        return Err(unknown(cx.session_mut(), None));
    };
    let candidates: Vec<Arc<dyn IdentifierResolver<L, T>>> = {
        let session = cx.session_mut();
        let Some(dispatch) = session.dispatch_registry_mut::<ReadDispatch<L, T>>(ordinal) else {
            return Err(unknown(session, Some(ordinal)));
        };
        if let Some(reader) = dispatch.readers.get(identifier) {
            return Ok(reader.clone());
        }
        // The matching resolvers, longest prefix first (stable, so registration order
        // decides among equal lengths); cloned out so the resolvers can use the context.
        let mut matching: Vec<(usize, Arc<dyn IdentifierResolver<L, T>>)> = dispatch
            .resolvers
            .iter()
            .filter(|(prefix, _)| identifier.starts_with(prefix.as_str()))
            .map(|(prefix, resolver)| (prefix.len(), Arc::clone(resolver)))
            .collect();
        matching.sort_by(|a, b| b.0.cmp(&a.0));
        matching.into_iter().map(|(_, resolver)| resolver).collect()
    };
    for resolver in candidates {
        if let Some(reader) = resolver.resolve(identifier, cx)? {
            let session = cx.session_mut();
            if let Some(dispatch) = session.dispatch_registry_mut::<ReadDispatch<L, T>>(ordinal) {
                dispatch.readers.entry(String::from(identifier)).or_insert_with(|| reader.clone());
            }
            return Ok(reader);
        }
    }
    Err(unknown(cx.session_mut(), Some(ordinal)))
}

impl<L, T, I> TableHandle<DispatchingSerdeDriver<L, T, I>>
where
    L: SerializableLang,
    T: ?Sized + SerializableObject<L> + Send + Sync + 'static,
    I: SerialIndex,
{
    /// Register the reader for `identifier` in this table of `session`, from `C`'s
    /// own [`DeserializableObject`] impl: entries with that identifier are rebuilt by
    /// `C::deserialize_object`, and `wrap` turns the result into the table's `Arc<T>`
    /// (see [`ObjectReader::from_type`]).
    ///
    /// # Errors
    ///
    /// [`RegistrationError::UnknownTable`] when the handle is not one of `session`'s;
    /// [`RegistrationError::DuplicateIdentifier`] when a reader for `identifier` is
    /// already registered (or a resolver's answer for it was kept).
    pub fn register_type<C: DeserializableObject<L>>(
        self,
        session: &mut SerdeSession<L>,
        identifier: impl Into<Cow<'static, str>>,
        wrap: impl Fn(C::Output) -> Arc<T> + Send + Sync + 'static,
    ) -> Result<(), RegistrationError> {
        self.register_reader(session, identifier, ObjectReader::from_type::<C>(wrap))
    }

    /// Register `reader` as the reader for `identifier` in this table of `session`.
    ///
    /// # Errors
    ///
    /// As [`register_type`](TableHandle::register_type).
    pub fn register_reader(
        self,
        session: &mut SerdeSession<L>,
        identifier: impl Into<Cow<'static, str>>,
        reader: ObjectReader<L, T>,
    ) -> Result<(), RegistrationError> {
        let ordinal = session.table_index(self).map_err(|table| RegistrationError::UnknownTable { table })?;
        let table = session.table_name(ordinal);
        let identifier: Cow<'static, str> = identifier.into();
        let dispatch = session
            .dispatch_registry_mut::<ReadDispatch<L, T>>(ordinal)
            .ok_or(RegistrationError::UnknownTable { table: self.id() })?;
        if dispatch.readers.contains_key(&*identifier) {
            return Err(RegistrationError::DuplicateIdentifier { table, identifier: identifier.into_owned() });
        }
        dispatch.readers.insert(identifier.into_owned(), reader);
        Ok(())
    }

    /// Register `resolver` for the identifiers beginning with `prefix` in this table
    /// of `session`. Several resolvers may be registered, with any prefixes (the empty
    /// prefix matches every identifier): for an identifier no registered reader
    /// covers, the resolvers whose prefix matches are asked in order of decreasing
    /// prefix length — the most specific first; registration order among equal
    /// lengths — until one answers; the answer is kept for the session's lifetime.
    ///
    /// # Errors
    ///
    /// [`RegistrationError::UnknownTable`] when the handle is not one of `session`'s.
    pub fn register_resolver(
        self,
        session: &mut SerdeSession<L>,
        prefix: impl Into<String>,
        resolver: Arc<dyn IdentifierResolver<L, T>>,
    ) -> Result<(), RegistrationError> {
        let ordinal = session.table_index(self).map_err(|table| RegistrationError::UnknownTable { table })?;
        let dispatch = session
            .dispatch_registry_mut::<ReadDispatch<L, T>>(ordinal)
            .ok_or(RegistrationError::UnknownTable { table: self.id() })?;
        dispatch.resolvers.push((prefix.into(), resolver));
        Ok(())
    }
}
