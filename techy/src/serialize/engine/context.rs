//! The contexts handed to serialization and deserialization calls:
//! [`SerializeContext`] and [`DeserializeContext`] — a call's access to the session
//! in progress.

use alloc::sync::Arc;
use core::any::Any;
use core::fmt;

use crate::engine::StdDescentGuard;

use super::super::error::{DeserializeError, SerializeError};
use super::super::object::SerializableLang;
use super::driver::{ObjectSerdeDriver, TableHandle};
use super::session::SerdeSession;

/// The serialization call's access to the session in progress — the `cx` argument of
/// [`serialize_object`](crate::serialize::SerializableObject::serialize_object),
/// [`serialize_argument_spec`](crate::core::specs::CallableSpec::serialize_argument_spec),
/// and [`ObjectSerdeDriver::serialize_object`].
///
/// A context exists only for a [`SerializableLang`]: that bound is what makes the
/// serialization methods, which take a context, callable exactly for languages that
/// support serialization. Constructed by the session only, for the duration of one
/// call.
pub struct SerializeContext<'a, L: SerializableLang> {
    session: &'a mut SerdeSession<L>,
    guard: &'a mut StdDescentGuard,
}

impl<'a, L: SerializableLang> SerializeContext<'a, L> {
    /// The context of one call, over the session and the run's descent guard.
    pub(crate) fn new(session: &'a mut SerdeSession<L>, guard: &'a mut StdDescentGuard) -> Self {
        SerializeContext { session, guard }
    }

    /// Intern `object` into the table `table`, returning its typed position: the
    /// object's serialized reference. An object already interned in the table (the
    /// same `Arc`, by pointer identity) yields its existing position without being
    /// serialized again — this is what preserves sharing; a new object is serialized
    /// by the table's driver (which may intern the objects it refers to, recursively)
    /// and appended to the table.
    ///
    /// # Errors
    ///
    /// The errors of [`SerdeSession::intern`], the same operation: the handle is not
    /// one of this session's ([`SerializeError::UnknownTable`]); the driver's failure,
    /// wrapped in [`SerializeError::InTable`]; the object refers back to itself
    /// ([`SerializeError::ReferenceCycle`]); the nesting exceeds the descent limit
    /// ([`SerializeError::DescentLimitExceeded`]); the entry carries an identifier
    /// other than a homogeneous table's ([`SerializeError::UnexpectedIdentifier`]) or
    /// nests too deep for a segment ([`SerializeError::Value`], wrapped in `InTable`);
    /// the table is full ([`SerializeError::TableFull`]).
    pub fn intern<D: ObjectSerdeDriver<L>>(
        &mut self,
        table: TableHandle<D>,
        object: &Arc<D::Object>,
    ) -> Result<D::Index, SerializeError> {
        self.session.intern_with_guard(self.guard, table, object)
    }

    /// The caller's user data of type `T`, if set on the session with
    /// [`SerdeSession::set_user_data`] (one value per type).
    pub fn user_data<T: Any>(&self) -> Option<&T> {
        self.session.user_data::<T>()
    }

    /// The handle of the session's table named `name`, if registered with driver type
    /// `D` (see [`SerdeSession::table_handle`]) — how a serialization call finds a
    /// table it holds no handle for.
    pub fn table_handle<D: ObjectSerdeDriver<L>>(&self, name: &str) -> Option<TableHandle<D>> {
        self.session.table_handle::<D>(name)
    }

    /// The session, for crate-internal operations (a driver's lookup of the
    /// registrations the session keeps for its table).
    pub(crate) fn session_mut(&mut self) -> &mut SerdeSession<L> {
        self.session
    }
}

impl<L: SerializableLang> fmt::Debug for SerializeContext<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SerializeContext").field("session", &self.session).finish_non_exhaustive()
    }
}

/// The deserialization call's access to the session in progress — the `cx` argument
/// of [`deserialize_object`](crate::serialize::DeserializableObject::deserialize_object),
/// [`deserialize_argument_spec`](crate::core::specs::CallableSpec::deserialize_argument_spec),
/// [`ObjectSerdeDriver::deserialize_object`], and
/// [`IdentifierResolver::resolve`](crate::serialize::IdentifierResolver::resolve).
///
/// A context exists only for a [`SerializableLang`]: that bound is what makes the
/// deserialization methods, which take a context, callable exactly for languages that
/// support serialization. Constructed by the session only, for the duration of one
/// call.
pub struct DeserializeContext<'a, L: SerializableLang> {
    session: &'a mut SerdeSession<L>,
    guard: &'a mut StdDescentGuard,
    /// The entry being deserialized by the call this context belongs to — `(table
    /// ordinal, position)` — or `None` for a context outside any entry (tests only).
    current: Option<(usize, u32)>,
}

impl<'a, L: SerializableLang> DeserializeContext<'a, L> {
    /// The context of one call, over the session and the run's descent guard; `current`
    /// is the entry the call deserializes.
    pub(crate) fn new(
        session: &'a mut SerdeSession<L>,
        guard: &'a mut StdDescentGuard,
        current: Option<(usize, u32)>,
    ) -> Self {
        DeserializeContext { session, guard, current }
    }

    /// The object stored at position `index` of table `table`, rebuilt from its entry
    /// if it has not been yet (a segment's entries are deserialized in table and
    /// position order, and any entry referred to before its turn is deserialized on
    /// demand). The same position always yields the same `Arc` — sharing survives
    /// the round trip. The position is one of this session's own (typically read out
    /// of the entry being deserialized, whose table references the session has
    /// already translated into its own numbering).
    ///
    /// # Errors
    ///
    /// The handle is not one of this session's ([`DeserializeError::UnknownTable`]);
    /// the position belongs to another table ([`DeserializeError::WrongTable`]) or
    /// lies beyond the table's end ([`DeserializeError::IndexOutOfRange`]); the entry
    /// refers back to itself ([`DeserializeError::ReferenceCycle`]); the nesting
    /// exceeds the descent limit ([`DeserializeError::DescentLimitExceeded`]); the
    /// driver's failure — or a heterogeneous table's entry that is not the
    /// identifier-and-data map ([`DeserializeError::Value`]) — wrapped in
    /// [`DeserializeError::InEntry`].
    pub fn object<D: ObjectSerdeDriver<L>>(
        &mut self,
        table: TableHandle<D>,
        index: D::Index,
    ) -> Result<Arc<D::Object>, DeserializeError> {
        self.session.object_with_guard(self.guard, table, index, self.current)
    }

    /// The caller's user data of type `T`, if set on the session with
    /// [`SerdeSession::set_user_data`] (one value per type) — the reading
    /// environment's entry point: how an implementation finds the live objects that
    /// serialized data refers to by identity (the crate's own providers through
    /// [`KnownProviders`](crate::serialize::KnownProviders), a framework's through
    /// its own environment type).
    pub fn user_data<T: Any>(&self) -> Option<&T> {
        self.session.user_data::<T>()
    }

    /// The handle of the session's table named `name`, if registered with driver type
    /// `D` (see [`SerdeSession::table_handle`]) — how a deserialization call finds a
    /// table it holds no handle for.
    pub fn table_handle<D: ObjectSerdeDriver<L>>(&self, name: &str) -> Option<TableHandle<D>> {
        self.session.table_handle::<D>(name)
    }

    /// The session, for crate-internal operations (the dispatching driver's reader
    /// lookup, a driver's lookup of the registrations the session keeps for its
    /// table).
    pub(crate) fn session_mut(&mut self) -> &mut SerdeSession<L> {
        self.session
    }
}

impl<L: SerializableLang> fmt::Debug for DeserializeContext<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeserializeContext")
            .field("session", &self.session)
            .field("current", &self.current)
            .finish_non_exhaustive()
    }
}
