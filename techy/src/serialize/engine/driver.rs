//! The per-table driver [`ObjectSerdeDriver`] — how the objects of one table are
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

/// How the objects of one table are serialized into entries and rebuilt from them.
///
/// A driver is what a table of a [`SerdeSession`](crate::serialize::SerdeSession) is
/// registered with
/// ([`register_table`](crate::serialize::SerdeSession::register_table)); the session
/// calls it for every object interned into the table and for every entry read back from
/// a segment.
///
/// There is one driver type per kind of table, and the method names are the same for
/// every kind: they mirror the object-level
/// [`SerializableObject::serialize_object`](crate::serialize::SerializableObject::serialize_object)
/// and
/// [`DeserializableObject::deserialize_object`](crate::serialize::DeserializableObject::deserialize_object),
/// the same operation at the table level.
///
/// A table holding objects of one kind only (a *homogeneous* table) implements the trait
/// directly, doing the work itself. A table holding trait objects of several concrete
/// types (a *heterogeneous* table) uses
/// [`DispatchingSerdeDriver`](crate::serialize::DispatchingSerdeDriver), which dispatches
/// on the object when writing and on the entry's identifier when reading.
///
/// `Send + Sync + 'static`: the session shares the driver behind an `Arc` and calls it
/// re-entrantly (an object's serialization may intern further objects of the same
/// table).
pub trait ObjectSerdeDriver<L: SerializableLang>: Send + Sync + 'static {
    /// The kind of object the table holds — a concrete type, or a trait object type
    /// (`dyn …`) for a heterogeneous table. Objects are shared with the session as
    /// `Arc<Self::Object>`.
    type Object: ?Sized + Send + Sync + 'static;

    /// The typed table position of this table — a type defined with
    /// [`serial_index!`](crate::serialize::serial_index).
    type Index: SerialIndex;

    /// The table's name: how a segment identifies the table, so that a reading session
    /// with a different registration order finds it.
    ///
    /// The name is a deliberately chosen, stable string owned by whoever defines the
    /// driver — the same stability obligation as an identifier.
    fn table_name(&self) -> &'static str;

    /// `Some(identifier)` for a homogeneous table, `None` for a heterogeneous one.
    ///
    /// The identifier is the one every entry of a homogeneous table has, which the table
    /// then does not write out, since the table itself implies the kind of object. The
    /// driver's [`serialize_object`](Self::serialize_object) still returns it in every
    /// entry, and the session reports a different one as an error. A heterogeneous
    /// table's entries record their identifier on the wire instead.
    ///
    /// `Some("")` is not an identifier: the session refuses to register such a driver
    /// ([`RegistrationError::EmptyHomogeneousIdentifier`](crate::serialize::RegistrationError::EmptyHomogeneousIdentifier)).
    fn homogeneous_identifier(&self) -> Option<&'static str>;

    /// Produces the entry for `object`.
    ///
    /// `cx` gives access to the session: interning the objects this one refers to
    /// ([`SerializeContext::intern`]) and the caller's user data.
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

    /// Rebuilds the object of `entry`.
    ///
    /// `entry.identifier` is the entry's identifier (the fixed one, for a homogeneous
    /// table) and `entry.data` its data. `cx` gives access to the session: reading the
    /// objects this one refers to ([`DeserializeContext::object`]) and the caller's user
    /// data.
    ///
    /// # Errors
    ///
    /// The entry is untrusted input: a value of the wrong shape, a reference that
    /// cannot be read (out of range, into the wrong table), an unknown identifier, or
    /// an object the reading environment lacks is an error, never a panic.
    fn deserialize_object(
        &self,
        entry: &SerialEntry,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Arc<Self::Object>, DeserializeError>;
}

/// The typed handle of a table registered in a
/// [`SerdeSession`](crate::serialize::SerdeSession).
///
/// A handle pairs the table's [`TableId`] with its driver type, so that interning into
/// and reading from the table are typed (`D::Object`, `D::Index`). It is returned by
/// [`SerdeSession::register_table`](crate::serialize::SerdeSession::register_table) and
/// found again by name with
/// [`SerdeSession::table_handle`](crate::serialize::SerdeSession::table_handle).
///
/// A handle is `Copy`, and meaningful only for the session that issued it: a session
/// validates every handle it is given — the table at that id must be registered with
/// driver type `D` — and otherwise fails the call with the `UnknownTable` error of its
/// kind.
///
/// The handle is also where a typed position is rebuilt from its bare `u32` index
/// ([`position`](TableHandle::position)): typed positions are scoped to the session that
/// minted them, so a position received from another session — whose table numbering may
/// differ — travels as its table's name and its `u32` index and is rebuilt on this side
/// (see [`SerialIndex`]).
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

    /// Returns the typed position `index` of this table, in the numbering of the session
    /// the handle belongs to.
    ///
    /// This is how a position received from elsewhere — from a writing session whose
    /// registration order may differ, or from a stored `u32` — is rebuilt for use with
    /// this session: `session.object(handle, handle.position(index))`. Positions travel
    /// between sessions as `(table name, u32)`, with [`SerialIndex::index`] on the
    /// sending side and this method on the receiving side, never as typed positions,
    /// which hold the minting session's [`TableId`].
    ///
    /// There is no bounds check here: the position is validated when it is used, where an
    /// index beyond the table's end is [`DeserializeError::IndexOutOfRange`].
    ///
    /// The language parameter `L` is inferred from the driver type's
    /// [`ObjectSerdeDriver`] impl; a driver implementing it for several languages needs
    /// it spelled out.
    pub fn position<L: SerializableLang>(self, index: u32) -> D::Index
    where
        D: ObjectSerdeDriver<L>,
    {
        D::Index::from_parts(self.id, index)
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
