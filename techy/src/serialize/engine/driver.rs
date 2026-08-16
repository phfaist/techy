//! [`ObjectSerdeDriver`] — the per-table driver: how the objects of one table are
//! serialized and deserialized — and [`TableHandle`], the typed handle a session
//! returns for a registered table.

use alloc::sync::Arc;
use core::any::TypeId;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;

use super::super::error::{DeserializeError, SerializeError};
use super::super::object::SerializableLang;
use super::super::value::{SerialEntry, SerialIndex, TableId};
use super::context::{DeserializeContext, SerializeContext};

/// The driver of one table of a [`SerdeSession`](crate::serialize::SerdeSession):
/// how the objects the table holds are serialized into entries and rebuilt from
/// them. Registered with
/// [`SerdeSession::register_table`](crate::serialize::SerdeSession::register_table);
/// the session calls the driver for every object interned into the table and for
/// every entry read back from a segment.
///
/// One driver type per kind of table; the method names are the same for every kind
/// (they mirror the object-level [`SerializableObject::serialize_object`](crate::serialize::SerializableObject::serialize_object)
/// and [`DeserializableObject::deserialize_object`](crate::serialize::DeserializableObject::deserialize_object):
/// the same operation at the table level). A table holding objects of one kind only
/// (a *homogeneous* table) implements the trait directly, doing the work itself; a
/// table holding trait objects of several concrete types (a *heterogeneous* table)
/// uses [`DispatchingSerdeDriver`](crate::serialize::DispatchingSerdeDriver), which
/// dispatches on the object (writing) and on the entry's identifier (reading).
///
/// `Send + Sync + 'static`: the session shares the driver behind an `Arc` and calls
/// it re-entrantly (an object's serialization may intern further objects of the same
/// table).
pub trait ObjectSerdeDriver<L: SerializableLang>: Send + Sync + 'static {
    /// The kind of object the table holds — a concrete type, or a trait object type
    /// (`dyn …`) for a heterogeneous table. Objects are shared with the session as
    /// `Arc<Self::Object>`.
    type Object: ?Sized + Send + Sync + 'static;

    /// The typed table position of this table — a type defined with
    /// [`serial_index!`](crate::serialize::serial_index).
    type Index: SerialIndex;

    /// The table's name: how a segment identifies the table, so that a reading
    /// session with a different registration order finds it. A deliberately chosen,
    /// stable string owned by whoever defines the driver — the same stability
    /// obligation as an identifier.
    fn table_name(&self) -> &'static str;

    /// For a homogeneous table, `Some(identifier)`: the identifier every entry of the
    /// table carries, which the table then does not write out (the table implies the
    /// kind of object) — the driver's [`serialize_object`](Self::serialize_object)
    /// still returns it in every entry, and the session reports a different one as an
    /// error. `None` for a heterogeneous table, whose entries carry their identifier
    /// on the wire.
    fn homogeneous_identifier(&self) -> Option<&'static str>;

    /// Produce the entry for `object`. `cx` gives access to the session: interning
    /// the objects this one refers to ([`SerializeContext::intern`]) and the caller's
    /// user data.
    ///
    /// # Errors
    ///
    /// The object cannot be serialized (its own
    /// [`serialize_object`](crate::serialize::SerializableObject::serialize_object)
    /// reports [`SerializeError::Unsupported`], or any failure of the driver or of the
    /// nested interning).
    fn serialize_object(
        &self,
        object: &Arc<Self::Object>,
        cx: &mut SerializeContext<'_, L>,
    ) -> Result<SerialEntry, SerializeError>;

    /// Rebuild the object of `entry`. `entry.identifier` is the entry's identifier
    /// (the fixed one, for a homogeneous table); `entry.data` its data. `cx` gives
    /// access to the session: resolving the entries this one refers to
    /// ([`DeserializeContext::resolve`]) and the caller's user data.
    ///
    /// # Errors
    ///
    /// The entry is untrusted input: a value of the wrong shape, a reference that
    /// cannot be resolved, an unknown identifier, or an object the reading environment
    /// lacks is an error, never a panic.
    fn deserialize_object(
        &self,
        entry: &SerialEntry,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Arc<Self::Object>, DeserializeError>;
}

/// The typed handle of a table registered in a
/// [`SerdeSession`](crate::serialize::SerdeSession): the table's [`TableId`] together
/// with its driver type, so that interning and resolving through it are typed
/// (`D::Object`, `D::Index`). Returned by
/// [`SerdeSession::register_table`](crate::serialize::SerdeSession::register_table);
/// `Copy`, and meaningful only for the session that issued it (a session validates
/// every handle it is given: the table at that id must be registered with driver type
/// `D`, else the call fails with the `UnknownTable` error of its kind).
pub struct TableHandle<D> {
    id: TableId,
    driver: PhantomData<fn() -> D>,
}

impl<D> TableHandle<D> {
    /// The handle for table `id` (the session validates the pairing on use).
    pub(crate) fn new(id: TableId) -> TableHandle<D> {
        TableHandle { id, driver: PhantomData }
    }

    /// The table's id: its ordinal in the session's registration order.
    pub fn id(self) -> TableId {
        self.id
    }

    /// The `TypeId` of the driver type — what a session compares its registration
    /// against.
    pub(crate) fn driver_type_id(self) -> TypeId
    where
        D: 'static,
    {
        TypeId::of::<D>()
    }
}

impl<D> Clone for TableHandle<D> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<D> Copy for TableHandle<D> {}

impl<D> PartialEq for TableHandle<D> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<D> Eq for TableHandle<D> {}

impl<D> Hash for TableHandle<D> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<D> fmt::Debug for TableHandle<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TableHandle")
            .field("id", &self.id)
            .field("driver", &core::any::type_name::<D>())
            .finish()
    }
}
