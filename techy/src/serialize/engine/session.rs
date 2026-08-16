//! [`SerdeSession`]: the tables, their entries, both direction maps (object → position
//! for writing, position → object for reading back), the registrations, and the
//! segment exchange.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::{Any, TypeId};
use core::fmt;
use core::marker::PhantomData;

use hashbrown::HashMap;

use crate::engine::{DescentGuard, StdDescentGuard, StdDescentGuardInit};

use super::super::error::{DeserializeError, RegistrationError, SerializeError};
use super::super::object::SerializableLang;
use super::super::value::{SerialEntry, SerialIndex, SerialValue, TableId};
use super::super::wire::{FromSerialValue, ToSerialValue};
use super::context::{DeserializeContext, SerializeContext};
use super::driver::{ObjectSerdeDriver, TableHandle};
use super::segment::{Segment, SegmentTable, WireEntry};

/// A serialization session: the tables of objects, read and write unified.
///
/// A session holds a set of *tables*, one per kind of object, each registered with
/// its [`ObjectSerdeDriver`] ([`register_table`](SerdeSession::register_table)) and
/// numbered in registration order ([`TableId`]). *Interning* an object into a table
/// ([`intern`](SerdeSession::intern), or [`SerializeContext::intern`] from inside a
/// serialization call) serializes it once and appends its entry to the table,
/// returning its typed position; interning the same object again — the same `Arc` —
/// returns the existing position, so shared objects are written once and referred to
/// by position, and sharing survives the round trip. Positions are stream-scoped and
/// append-only: a table only ever grows.
///
/// A [`Segment`] is what a session emits and absorbs. [`take_segment`](SerdeSession::take_segment)
/// packages every entry interned since the previous emission (and nothing older);
/// [`push_segment`](SerdeSession::push_segment) absorbs a segment into the reading
/// session's tables, validating it and rebuilding every entry's object through the
/// table's driver. A *stream* is the sequence of segments one writing session emits;
/// a reading session absorbs them in order and can then intern further objects and
/// emit segments of its own that refer back to what it absorbed — reading then
/// appending is the natural flow, and both directions share the same tables.
///
/// The session also carries the caller's *user data*
/// ([`set_user_data`](SerdeSession::set_user_data)) — the reading environment's
/// entry point: an implementation's `deserialize_object` reads it through the
/// context to find the live objects that serialized data refers to by identity — and
/// the readers and resolvers registered on heterogeneous tables (see
/// [`DispatchingSerdeDriver`](crate::serialize::DispatchingSerdeDriver)).
///
/// Nested calls — an object's serialization interning the objects it refers to, an
/// entry's deserialization reading the objects it refers to — are bounded by the
/// crate's descent guard ([`with_descent_guard_init`](SerdeSession::with_descent_guard_init));
/// reference cycles are detected on both sides and reported as errors, never
/// followed.
///
/// A session exists only for a [`SerializableLang`]. This constructor,
/// [`empty`](SerdeSession::empty), starts with no tables; a constructor with the
/// standard tables of the crate's own object kinds pre-registered is not part of the
/// crate at this stage.
///
/// # Example
///
/// A table of one kind of object, written by one session and read by another:
///
/// ```
/// use std::sync::Arc;
/// use techy::core::TrivialLang;
/// use techy::serialize::{
///     DeserializeContext, DeserializeError, ObjectSerdeDriver, SerdeSession, SerialEntry,
///     SerialValue, SerializableLang, SerializeContext, SerializeError,
/// };
///
/// #[derive(Debug, Clone, Copy)]
/// struct MyLang;
/// impl TrivialLang for MyLang {}
/// impl SerializableLang for MyLang {}
///
/// // The object kind, its position type, and its driver.
/// struct Label(String);
/// techy::serial_index! { pub struct LabelIndex; }
/// struct LabelDriver;
///
/// impl ObjectSerdeDriver<MyLang> for LabelDriver {
///     type Object = Label;
///     type Index = LabelIndex;
///     fn table_name(&self) -> &'static str { "labels" }
///     fn homogeneous_identifier(&self) -> Option<&'static str> { Some("example.label") }
///     fn serialize_object(&self, label: &Arc<Label>, _cx: &mut SerializeContext<'_, MyLang>)
///         -> Result<SerialEntry, SerializeError>
///     {
///         Ok(SerialEntry { identifier: "example.label".into(), data: SerialValue::Str(label.0.clone()) })
///     }
///     fn deserialize_object(&self, entry: &SerialEntry, _cx: &mut DeserializeContext<'_, MyLang>)
///         -> Result<Arc<Label>, DeserializeError>
///     {
///         match &entry.data {
///             SerialValue::Str(text) => Ok(Arc::new(Label(text.clone()))),
///             _ => Err(DeserializeError::failed("a label is a string")),
///         }
///     }
/// }
///
/// // Write: intern two objects (one of them twice) and emit a segment.
/// let mut writer = SerdeSession::<MyLang>::empty();
/// let labels = writer.register_table(LabelDriver)?;
/// let shared = Arc::new(Label("shared".into()));
/// let first = writer.intern(labels, &shared)?;
/// let second = writer.intern(labels, &Arc::new(Label("other".into())))?;
/// assert_eq!(writer.intern(labels, &shared)?, first);   // the same position again
/// let segment = writer.take_segment();
///
/// // Read: absorb the segment into another session and read the objects back. The
/// // reader registered its tables in the writer's order, so the writer's positions
/// // are valid in it as they are (see `TableHandle::position` for the general case).
/// let mut reader = SerdeSession::<MyLang>::empty();
/// let labels = reader.register_table(LabelDriver)?;
/// reader.push_segment(segment)?;
/// assert_eq!(reader.object(labels, first)?.0, "shared");
/// assert_eq!(reader.object(labels, second)?.0, "other");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct SerdeSession<L: SerializableLang> {
    tables: Vec<TableState<L>>,
    user_data: Option<Box<dyn Any + Send + Sync>>,
    descent_guard_init: StdDescentGuardInit,
    /// The write side's in-progress stack: `(table ordinal, object address)` of the
    /// objects whose serialization is in progress, outermost first — the write-side
    /// cycle check.
    write_stack: Vec<(usize, usize)>,
    /// The resolver answers memoized during the segment push in progress —
    /// `(table ordinal, identifier)` — forgotten again if the push fails (part of the
    /// rollback). Entries are rebuilt only inside a push, so this is where every
    /// memoization happens.
    memo_journal: Vec<(usize, String)>,
    lang: PhantomData<fn() -> L>,
}

/// One table of the session.
pub(super) struct TableState<L: SerializableLang> {
    name: &'static str,
    /// The fixed identifier of a homogeneous table (`None`: heterogeneous).
    homogeneous: Option<&'static str>,
    /// The driver, `Arc<D>` behind `dyn Any` (the session is type-blind; typed access
    /// downcasts).
    driver: Arc<dyn Any + Send + Sync>,
    /// `TypeId::of::<D>()`: what a handle is validated against.
    driver_type: TypeId,
    /// The typed materialization routine for this table's driver type.
    materialize: MaterializeFn<L>,
    /// The entries, by position.
    slots: Vec<Slot>,
    /// Object address → position: the write side's interning map (and the read side's
    /// record of rebuilt objects, so that re-interning one yields its position).
    by_pointer: HashMap<usize, u32>,
    /// The serialized forms of the entries interned since the previous emission, in
    /// position order — always the tail of `slots`.
    outbox: Vec<SerialValue>,
    /// The readers and resolvers of a heterogeneous table (a `ReadDispatch<L, T>`),
    /// created on first use.
    pub(super) dispatch: Option<Box<dyn ReadDispatchState>>,
}

/// The read-side registrations of a heterogeneous table — the readers by identifier,
/// the resolvers, the memoized resolver answers — as the type-blind session holds
/// them: created on first use through
/// [`dispatch_registry_mut`](SerdeSession::dispatch_registry_mut), which downcasts to
/// the concrete registry type, and asked to forget memoized answers when a push is
/// rolled back.
pub(super) trait ReadDispatchState: Any + Send + Sync {
    /// The registry as `dyn Any`, for the typed downcast.
    fn as_any_mut(&mut self) -> &mut (dyn Any + Send + Sync);

    /// Forget the memoized resolver answer for `identifier`, if any.
    fn forget_memo(&mut self, identifier: &str);
}

/// One entry of a table.
enum Slot {
    /// Absorbed from a segment, not yet rebuilt: the stored serialized form.
    Pending(SerialValue),
    /// Being rebuilt right now (the read side's cycle check).
    InProgress,
    /// The live object, `Arc<D::Object>` behind `dyn Any`.
    Materialized(Box<dyn Any + Send + Sync>),
}

impl Slot {
    fn is_materialized(&self) -> bool {
        matches!(self, Slot::Materialized(_))
    }
}

/// The erased entry point of [`materialize`]: rebuild one pending entry.
type MaterializeFn<L> = fn(
    &mut SerdeSession<L>,
    &mut StdDescentGuard,
    usize,
    u32,
    Option<(usize, u32)>,
) -> Result<(), DeserializeError>;

/// The address of an `Arc`'s allocation, as the interning key: object identity. The
/// `Arc` is kept alive in the table, so the address cannot be reused while the map
/// holds it.
fn address<T: ?Sized>(object: &Arc<T>) -> usize {
    Arc::as_ptr(object) as *const () as usize
}

impl<L: SerializableLang> SerdeSession<L> {
    /// A session with no tables; register them with
    /// [`register_table`](SerdeSession::register_table).
    pub fn empty() -> SerdeSession<L> {
        SerdeSession {
            tables: Vec::new(),
            user_data: None,
            descent_guard_init: StdDescentGuardInit::default(),
            write_stack: Vec::new(),
            memo_journal: Vec::new(),
            lang: PhantomData,
        }
    }

    /// Configure the descent guard that bounds nested calls — an object's
    /// serialization interning the objects it refers to, an entry's deserialization
    /// reading the objects it refers to, each level one *descent* — for every
    /// subsequent [`intern`](SerdeSession::intern), [`object`](SerdeSession::object),
    /// and [`push_segment`](SerdeSession::push_segment) call (each is one run with a
    /// fresh guard). The default is the crate-wide default ([`StdDescentGuardInit`]:
    /// a stack budget). The guard's early warning has no observer here and is
    /// dropped; a refusal is a `DescentLimitExceeded` error.
    pub fn with_descent_guard_init(mut self, init: StdDescentGuardInit) -> SerdeSession<L> {
        self.descent_guard_init = init;
        self
    }

    // --- registration -----------------------------------------------------------------

    /// Register a table with its driver, returning the typed handle to intern and
    /// read through. Tables are numbered in registration order; the id is the
    /// handle's [`id`](TableHandle::id).
    ///
    /// # Errors
    ///
    /// [`RegistrationError::DuplicateTableName`] when a table of the driver's
    /// [`table_name`](ObjectSerdeDriver::table_name) is already registered;
    /// [`RegistrationError::EmptyHomogeneousIdentifier`] when the driver's
    /// [`homogeneous_identifier`](ObjectSerdeDriver::homogeneous_identifier) is
    /// `Some("")` (an identifier is a real, non-empty string);
    /// [`RegistrationError::TooManyTables`] at `u32::MAX` tables.
    pub fn register_table<D: ObjectSerdeDriver<L>>(
        &mut self,
        driver: D,
    ) -> Result<TableHandle<D>, RegistrationError> {
        let name = driver.table_name();
        if self.tables.iter().any(|table| table.name == name) {
            return Err(RegistrationError::DuplicateTableName { name });
        }
        if driver.homogeneous_identifier() == Some("") {
            return Err(RegistrationError::EmptyHomogeneousIdentifier { table: name });
        }
        let ordinal = u32::try_from(self.tables.len()).map_err(|_| RegistrationError::TooManyTables)?;
        if ordinal == u32::MAX {
            return Err(RegistrationError::TooManyTables);
        }
        self.tables.push(TableState {
            name,
            homogeneous: driver.homogeneous_identifier(),
            driver: Arc::new(driver),
            driver_type: TypeId::of::<D>(),
            materialize: materialize_erased::<L, D>,
            slots: Vec::new(),
            by_pointer: HashMap::new(),
            outbox: Vec::new(),
            dispatch: None,
        });
        Ok(TableHandle::new(TableId::new(ordinal)))
    }

    /// The ordinal of the table `handle` names, if the handle is one of this
    /// session's: the table exists and is registered with driver type `D`. `Err` is
    /// the handle's id, for the caller's `UnknownTable` error.
    pub(super) fn table_index<D: 'static>(&self, handle: TableHandle<D>) -> Result<usize, TableId> {
        let ordinal = usize::try_from(handle.id().ordinal()).map_err(|_| handle.id())?;
        match self.tables.get(ordinal) {
            Some(table) if table.driver_type == handle.driver_type_id() => Ok(ordinal),
            _ => Err(handle.id()),
        }
    }

    /// The name of table `ordinal` (a valid ordinal).
    pub(super) fn table_name(&self, ordinal: usize) -> &'static str {
        self.tables.get(ordinal).map_or("?", |table| table.name)
    }

    /// The ordinal of the table named `name`, if registered.
    pub(super) fn table_ordinal_by_name(&self, name: &str) -> Option<usize> {
        self.tables.iter().position(|table| table.name == name)
    }

    /// The table `ordinal`'s dispatch registry, typed — created on first use. `None`
    /// when the table's registry was created for another object type.
    pub(super) fn dispatch_registry_mut<R: ReadDispatchState + Default>(
        &mut self,
        ordinal: usize,
    ) -> Option<&mut R> {
        let table = self.tables.get_mut(ordinal)?;
        table
            .dispatch
            .get_or_insert_with(|| Box::new(R::default()) as Box<dyn ReadDispatchState>)
            .as_any_mut()
            .downcast_mut::<R>()
    }

    /// Record that the resolver answer for `identifier` in table `ordinal` was
    /// memoized during the push in progress (forgotten again if the push fails).
    pub(super) fn record_memo(&mut self, ordinal: usize, identifier: &str) {
        self.memo_journal.push((ordinal, String::from(identifier)));
    }

    // --- user data ----------------------------------------------------------------------

    /// Set the caller's user data: any value, replacing the previous one. It is what
    /// serialization and deserialization calls see through their context's
    /// `user_data` — the reading environment (the live objects that serialized data
    /// refers to by identity), or anything else an implementation needs.
    pub fn set_user_data<T: Any + Send + Sync>(&mut self, data: T) {
        self.user_data = Some(Box::new(data));
    }

    /// The caller's user data, if set and of type `T`.
    pub fn user_data<T: Any>(&self) -> Option<&T> {
        self.user_data.as_ref()?.downcast_ref::<T>()
    }

    // --- writing ------------------------------------------------------------------------

    /// Intern `object` into the table `table`, returning its typed position — the
    /// top-level entry point of the write side ([`SerializeContext::intern`] is the
    /// same operation from inside a serialization call). An object already interned
    /// in the table (the same `Arc`, by pointer identity) yields its existing position
    /// without being serialized again; a new object is serialized by the table's
    /// driver — which may intern the objects it refers to, recursively — and appended
    /// to the table, after the objects it interned (so every reference points to an
    /// earlier position). Objects interned by a failed call's nested interning stay
    /// interned.
    ///
    /// # Errors
    ///
    /// The handle is not one of this session's ([`SerializeError::UnknownTable`]);
    /// the driver's failure, wrapped in [`SerializeError::InTable`]; the object refers
    /// back to itself ([`SerializeError::ReferenceCycle`]); the nesting exceeds the
    /// descent limit ([`SerializeError::DescentLimitExceeded`]); the table is full
    /// ([`SerializeError::TableFull`]).
    pub fn intern<D: ObjectSerdeDriver<L>>(
        &mut self,
        table: TableHandle<D>,
        object: &Arc<D::Object>,
    ) -> Result<D::Index, SerializeError> {
        let mut guard = StdDescentGuard::init(&self.descent_guard_init);
        self.intern_with_guard(&mut guard, table, object)
    }

    /// [`intern`](SerdeSession::intern) within a run whose guard is `guard`.
    pub(super) fn intern_with_guard<D: ObjectSerdeDriver<L>>(
        &mut self,
        guard: &mut StdDescentGuard,
        table: TableHandle<D>,
        object: &Arc<D::Object>,
    ) -> Result<D::Index, SerializeError> {
        let ordinal = self.table_index(table).map_err(|table| SerializeError::UnknownTable { table })?;
        let table_id = TableId::new(ordinal as u32);
        let name = self.tables[ordinal].name;
        let pointer = address(object);
        if let Some(&position) = self.tables[ordinal].by_pointer.get(&pointer) {
            return Ok(D::Index::from_parts(table_id, position));
        }
        if self.write_stack.iter().any(|&(t, p)| t == ordinal && p == pointer) {
            // The innermost in-progress object is the one whose serialization interned
            // this one again.
            let referrer = self.write_stack.last().map_or(name, |&(t, _)| self.tables[t].name);
            return Err(SerializeError::ReferenceCycle { table: name, referrer });
        }
        // The descent: the driver call, with the guard's permission. Warnings have no
        // observer here.
        if let Err(refusal) = guard.try_enter() {
            return Err(SerializeError::DescentLimitExceeded { detail: refusal.detail });
        }
        self.write_stack.push((ordinal, pointer));
        let result = match self.driver::<D>(ordinal) {
            Some(driver) => {
                let mut cx = SerializeContext::new(self, guard);
                driver.serialize_object(object, &mut cx)
            }
            None => Err(SerializeError::UnknownTable { table: table_id }),
        };
        self.write_stack.pop();
        guard.exit();
        let entry = result.map_err(|error| error.in_table(name))?;

        // Post-order: the position is assigned after the objects this one interned.
        let state = &mut self.tables[ordinal];
        let wire = match state.homogeneous {
            Some(expected) => {
                if entry.identifier != expected {
                    return Err(SerializeError::UnexpectedIdentifier {
                        table: name,
                        expected,
                        found: String::from(&*entry.identifier),
                    });
                }
                entry.data
            }
            None => WireEntry { identifier: entry.identifier, data: entry.data }.to_serial_value()?,
        };
        let position = u32::try_from(state.slots.len()).map_err(|_| SerializeError::TableFull { table: name })?;
        if position == u32::MAX {
            return Err(SerializeError::TableFull { table: name });
        }
        state.slots.push(Slot::Materialized(Box::new(object.clone())));
        state.by_pointer.insert(pointer, position);
        state.outbox.push(wire);
        Ok(D::Index::from_parts(table_id, position))
    }

    /// The driver of table `ordinal` as `Arc<D>` (a clone, so the session is free
    /// while the driver runs); `None` if the table's driver is not a `D`.
    fn driver<D: ObjectSerdeDriver<L>>(&self, ordinal: usize) -> Option<Arc<D>> {
        Arc::clone(&self.tables.get(ordinal)?.driver).downcast::<D>().ok()
    }

    /// Emit a segment: every entry interned into any table since the previous
    /// emission (or since the session was created), and nothing older — the
    /// entries absorbed with [`push_segment`](SerdeSession::push_segment) are never
    /// re-emitted. Every registered table appears in the segment, in registration
    /// order, with the position its entries start at (so a reading session can check
    /// it continues the stream) — with no entries if nothing new was interned into it.
    /// The segment's version is [`Segment::VERSION`].
    ///
    /// The output is deterministic: entries in position order, tables in registration
    /// order; two sessions that intern the same objects in the same order emit equal
    /// segments.
    pub fn take_segment(&mut self) -> Segment {
        let tables = self
            .tables
            .iter_mut()
            .enumerate()
            .map(|(ordinal, table)| {
                let entries = core::mem::take(&mut table.outbox);
                // Invariant: the outbox is the tail of the slots (every interned entry
                // pushes both), so the entries start where the slots minus the outbox
                // end; slot counts stay below `u32::MAX` (the table-full checks).
                debug_assert!(entries.len() <= table.slots.len(), "the outbox is the tail of the slots");
                let start = table.slots.len().saturating_sub(entries.len()) as u32;
                SegmentTable::new(String::from(table.name), TableId::new(ordinal as u32), start, entries)
            })
            .collect();
        Segment::new(Segment::VERSION, tables)
    }

    // --- reading ------------------------------------------------------------------------

    /// Absorb a segment: append its entries to the session's tables and rebuild every
    /// object, so that they can be read back ([`object`](SerdeSession::object)) and
    /// interned again (an absorbed object interned into its table yields its existing
    /// position — no re-emission).
    ///
    /// The segment is validated as untrusted input, then materialized: its version
    /// must be [`Segment::VERSION`]; each of its tables is matched to a table of this
    /// session *by name* (registration order may differ between writer and reader),
    /// and its entries must start exactly where the session's table ends — segments
    /// of a stream are pushed in order, each once; every table reference in the
    /// entries is translated from the writer's table numbering to this session's,
    /// through the segment's table directory; then every new entry is rebuilt by its
    /// table's driver, table by table in registration order and entry by entry in
    /// position order (an entry referred to before its turn is rebuilt on demand).
    /// The session must have no entries pending emission (interned since the last
    /// [`take_segment`](SerdeSession::take_segment)): a segment continues the stream
    /// the session has emitted so far.
    ///
    /// **The segments pushed into one session must all come from one stream, in
    /// order** — the caller's obligation. The session checks that each segment
    /// continues its tables (the start positions), which catches a skipped or
    /// repeated segment; it cannot recognize a segment of another stream whose
    /// positions happen to line up, and would absorb it as a continuation.
    ///
    /// On any error the session is left exactly as it was before the call — the
    /// segment's entries are dropped, and the session stays usable — and the error
    /// names the culprit.
    ///
    /// # Errors
    ///
    /// [`DeserializeError::UnsupportedVersion`], [`UnknownTableName`](DeserializeError::UnknownTableName),
    /// [`DuplicateSegmentTable`](DeserializeError::DuplicateSegmentTable),
    /// [`SegmentOutOfOrder`](DeserializeError::SegmentOutOfOrder),
    /// [`UnemittedEntries`](DeserializeError::UnemittedEntries),
    /// [`TableFull`](DeserializeError::TableFull), and
    /// [`UnknownWriterTable`](DeserializeError::UnknownWriterTable) for a segment that
    /// does not fit the session; then, from rebuilding the entries, a driver's failure
    /// wrapped in [`InEntry`](DeserializeError::InEntry), a reference cycle
    /// ([`ReferenceCycle`](DeserializeError::ReferenceCycle)), a reference beyond a
    /// table's end ([`IndexOutOfRange`](DeserializeError::IndexOutOfRange)) or into
    /// the wrong table ([`WrongTable`](DeserializeError::WrongTable)), or the descent
    /// limit ([`DescentLimitExceeded`](DeserializeError::DescentLimitExceeded)); and
    /// [`Internal`](DeserializeError::Internal) should the session's own check after
    /// rebuilding — every new entry holds its object — fail (a bug of this crate).
    pub fn push_segment(&mut self, segment: Segment) -> Result<(), DeserializeError> {
        let (version, tables) = segment.into_parts();
        if version != Segment::VERSION {
            return Err(DeserializeError::UnsupportedVersion { found: version, expected: Segment::VERSION });
        }

        // A segment continues the stream the session has emitted so far: nothing
        // interned since the last emission may still be pending (absorb every segment
        // first, then append and emit — the read-then-append flow).
        if let Some(table) = self.tables.iter().find(|table| !table.outbox.is_empty()) {
            return Err(DeserializeError::UnemittedEntries { table: table.name });
        }

        // Validate the table directory against this session, and plan the appends,
        // before touching any state.
        let mut writer_to_reader: HashMap<u32, usize> = HashMap::new();
        let mut planned: Vec<(usize, Vec<SerialValue>)> = Vec::with_capacity(tables.len());
        for segment_table in tables {
            let (name, id, start, entries) = segment_table.into_parts();
            let ordinal = self
                .tables
                .iter()
                .position(|table| table.name == name)
                .ok_or_else(|| DeserializeError::UnknownTableName { name: name.clone() })?;
            if planned.iter().any(|(t, _)| *t == ordinal) {
                return Err(DeserializeError::DuplicateSegmentTable { name });
            }
            if writer_to_reader.insert(id.ordinal(), ordinal).is_some() {
                return Err(DeserializeError::DuplicateSegmentTable { name });
            }
            let table = &self.tables[ordinal];
            let len = table.slots.len() as u32;
            if start != len {
                return Err(DeserializeError::SegmentOutOfOrder { table: table.name, expected: len, found: start });
            }
            let fits = u32::try_from(entries.len()).ok().and_then(|n| len.checked_add(n));
            match fits {
                Some(end) if end < u32::MAX => {}
                _ => return Err(DeserializeError::TableFull { table: table.name }),
            }
            planned.push((ordinal, entries));
        }

        // Translate every table reference into this session's numbering.
        for (_, entries) in &mut planned {
            for entry in entries.iter_mut() {
                translate_indices(entry, &writer_to_reader)?;
            }
        }

        // Append as pending, remembering the old lengths for the rollback.
        let old_lens: Vec<usize> = self.tables.iter().map(|table| table.slots.len()).collect();
        for (ordinal, entries) in planned {
            self.tables[ordinal].slots.extend(entries.into_iter().map(Slot::Pending));
        }

        // Rebuild every new entry, eagerly and in order.
        self.memo_journal.clear();
        let mut guard = StdDescentGuard::init(&self.descent_guard_init);
        let result = self.materialize_new_entries(&mut guard, &old_lens);
        let memoized = core::mem::take(&mut self.memo_journal);
        if result.is_err() {
            for (table, &old_len) in self.tables.iter_mut().zip(&old_lens) {
                table.slots.truncate(old_len);
                let old_len = old_len as u32;
                table.by_pointer.retain(|_, position| *position < old_len);
            }
            // The resolver answers recorded during the push are part of what it did.
            for (ordinal, identifier) in memoized {
                if let Some(dispatch) = self.tables.get_mut(ordinal).and_then(|table| table.dispatch.as_mut()) {
                    dispatch.forget_memo(&identifier);
                }
            }
        }
        result
    }

    /// Rebuild every entry beyond `old_lens`, table by table, position by position.
    /// Returns `Ok` only when every new entry is rebuilt: an entry whose rebuilding
    /// failed is left pending (see [`materialize_entered`]), so a failure swallowed by
    /// the driver of an entry that referred to it — a driver treating a reference as
    /// optional — is met again here, in the entry's own turn, and fails the push.
    fn materialize_new_entries(
        &mut self,
        guard: &mut StdDescentGuard,
        old_lens: &[usize],
    ) -> Result<(), DeserializeError> {
        for (ordinal, &old_len) in old_lens.iter().enumerate() {
            let mut position = old_len;
            while position < self.tables[ordinal].slots.len() {
                if let Slot::Pending(_) = self.tables[ordinal].slots[position] {
                    let materialize = self.tables[ordinal].materialize;
                    materialize(self, guard, ordinal, position as u32, None)?;
                }
                position += 1;
            }
        }
        // Defense in depth: after the pass, every new slot holds its object (a
        // rebuilding that failed has returned the error above; none is still in
        // progress once every call has returned). A slot found otherwise is a bug of
        // this crate, reported rather than left to misreport later.
        for (ordinal, &old_len) in old_lens.iter().enumerate() {
            let table = &self.tables[ordinal];
            if let Some(offset) = table.slots[old_len..].iter().position(|slot| !slot.is_materialized()) {
                return Err(DeserializeError::Internal {
                    detail: alloc::format!(
                        "entry #{} of table `{}` was left unrebuilt after the segment's entries were processed",
                        old_len.saturating_add(offset),
                        table.name
                    ),
                });
            }
        }
        Ok(())
    }

    /// The object stored at position `index` of table `table` — the top-level entry
    /// point of the read side ([`DeserializeContext::object`] is the same operation
    /// from inside a deserialization call). Every entry of an absorbed segment is
    /// rebuilt when the segment is pushed, so this is a lookup; the same position
    /// always yields the same `Arc`.
    ///
    /// The position must be one of this session's own: a typed position carries the
    /// [`TableId`] of the session that minted it (see [`SerialIndex`]), and a
    /// position minted by another session — a writing session whose registration
    /// order differs — names the wrong table here. Between sessions a position is
    /// exchanged as its table's name and its bare `u32` index
    /// ([`SerialIndex::index`]), and rebuilt on this side with
    /// [`TableHandle::position`].
    ///
    /// # Errors
    ///
    /// The handle is not one of this session's ([`DeserializeError::UnknownTable`]);
    /// the position belongs to another table ([`DeserializeError::WrongTable`]) or
    /// lies beyond the table's end ([`DeserializeError::IndexOutOfRange`]).
    pub fn object<D: ObjectSerdeDriver<L>>(
        &mut self,
        table: TableHandle<D>,
        index: D::Index,
    ) -> Result<Arc<D::Object>, DeserializeError> {
        let mut guard = StdDescentGuard::init(&self.descent_guard_init);
        self.object_with_guard(&mut guard, table, index, None)
    }

    /// [`object`](SerdeSession::object) within a run whose guard is `guard`, from
    /// the entry `referrer` (the entry being deserialized), if any.
    pub(super) fn object_with_guard<D: ObjectSerdeDriver<L>>(
        &mut self,
        guard: &mut StdDescentGuard,
        table: TableHandle<D>,
        index: D::Index,
        referrer: Option<(usize, u32)>,
    ) -> Result<Arc<D::Object>, DeserializeError> {
        let ordinal = self.table_index(table).map_err(|table| DeserializeError::UnknownTable { table })?;
        let name = self.tables[ordinal].name;
        if index.table() != table.id() {
            return Err(DeserializeError::WrongTable { expected: name, found: index.table() });
        }
        let position = index.index();
        let len = self.tables[ordinal].slots.len() as u32;
        if position >= len {
            return Err(DeserializeError::IndexOutOfRange { table: name, index: position, len });
        }
        materialize::<L, D>(self, guard, ordinal, position, referrer)
    }

    /// The number of entries of table `ordinal` (crate-internal, for tests).
    #[cfg(test)]
    pub(super) fn table_len(&self, ordinal: usize) -> usize {
        self.tables[ordinal].slots.len()
    }
}

/// Rebuild the entry at `position` of table `ordinal` (whose driver is `D`) if it is
/// pending, and return its object; `referrer` is the entry whose deserialization
/// asked for it, for the cycle error.
fn materialize<L: SerializableLang, D: ObjectSerdeDriver<L>>(
    session: &mut SerdeSession<L>,
    guard: &mut StdDescentGuard,
    ordinal: usize,
    position: u32,
    referrer: Option<(usize, u32)>,
) -> Result<Arc<D::Object>, DeserializeError> {
    let name = session.tables[ordinal].name;
    let table_id = TableId::new(ordinal as u32);
    match session.tables[ordinal].slots.get(position as usize) {
        Some(Slot::Materialized(object)) => {
            return object
                .downcast_ref::<Arc<D::Object>>()
                .cloned()
                .ok_or(DeserializeError::UnknownTable { table: table_id });
        }
        Some(Slot::InProgress) => {
            let (referrer_table, referrer_index) = referrer.unwrap_or((ordinal, position));
            return Err(DeserializeError::ReferenceCycle {
                table: name,
                index: position,
                referrer_table: session.table_name(referrer_table),
                referrer_index,
            });
        }
        Some(Slot::Pending(_)) => {}
        None => {
            let len = session.tables[ordinal].slots.len() as u32;
            return Err(DeserializeError::IndexOutOfRange { table: name, index: position, len });
        }
    }
    // The descent: the driver call, with the guard's permission. Warnings have no
    // observer here.
    if let Err(refusal) = guard.try_enter() {
        return Err(DeserializeError::DescentLimitExceeded { detail: refusal.detail });
    }
    let result = materialize_entered::<L, D>(session, guard, ordinal, position);
    guard.exit();
    result
}

/// The granted-descent body of [`materialize`]: take the pending value out, rebuild,
/// store. On any failure the slot is put back as pending, with its stored value: a
/// failed rebuilding leaves no trace (the slot is neither poisoned as in-progress —
/// which would misreport as a cycle later — nor emptied), so that a later attempt
/// meets the entry again exactly as it was.
fn materialize_entered<L: SerializableLang, D: ObjectSerdeDriver<L>>(
    session: &mut SerdeSession<L>,
    guard: &mut StdDescentGuard,
    ordinal: usize,
    position: u32,
) -> Result<Arc<D::Object>, DeserializeError> {
    let name = session.tables[ordinal].name;
    let table_id = TableId::new(ordinal as u32);
    let slot = &mut session.tables[ordinal].slots[position as usize];
    let value = match core::mem::replace(slot, Slot::InProgress) {
        Slot::Pending(value) => value,
        // Checked by the caller; restore and report rather than assume.
        other => {
            *slot = other;
            return Err(DeserializeError::IndexOutOfRange { table: name, index: position, len: 0 });
        }
    };
    // The entry handed to the driver, and — for a heterogeneous table, whose stored
    // form the entry is parsed out of — the stored value itself, kept to restore.
    let (entry, stored) = match session.tables[ordinal].homogeneous {
        Some(identifier) => (SerialEntry { identifier: Cow::Borrowed(identifier), data: value }, None),
        None => match WireEntry::from_serial_value(&value) {
            Ok(wire) => (SerialEntry { identifier: wire.identifier, data: wire.data }, Some(value)),
            Err(error) => {
                session.tables[ordinal].slots[position as usize] = Slot::Pending(value);
                return Err(DeserializeError::from(error).in_entry(name, position, None));
            }
        },
    };
    let Some(driver) = session.driver::<D>(ordinal) else {
        let SerialEntry { data, .. } = entry;
        session.tables[ordinal].slots[position as usize] = Slot::Pending(stored.unwrap_or(data));
        return Err(DeserializeError::UnknownTable { table: table_id });
    };
    let result = {
        let mut cx = DeserializeContext::new(session, guard, Some((ordinal, position)));
        driver.deserialize_object(&entry, &mut cx)
    };
    match result {
        Ok(object) => {
            let table = &mut session.tables[ordinal];
            table.slots[position as usize] = Slot::Materialized(Box::new(object.clone()));
            table.by_pointer.entry(address(&object)).or_insert(position);
            Ok(object)
        }
        Err(error) => {
            let SerialEntry { identifier, data } = entry;
            session.tables[ordinal].slots[position as usize] = Slot::Pending(stored.unwrap_or(data));
            Err(error.in_entry(name, position, Some(identifier)))
        }
    }
}

/// [`materialize`] as the erased routine stored per table.
fn materialize_erased<L: SerializableLang, D: ObjectSerdeDriver<L>>(
    session: &mut SerdeSession<L>,
    guard: &mut StdDescentGuard,
    ordinal: usize,
    position: u32,
    referrer: Option<(usize, u32)>,
) -> Result<(), DeserializeError> {
    materialize::<L, D>(session, guard, ordinal, position, referrer).map(|_| ())
}

/// Rewrite every `Index` inside `value` from the writer's table numbering to the
/// reader's, through `writer_to_reader`. Iterative, so a deeply nested value cannot
/// exhaust the stack.
fn translate_indices(
    value: &mut SerialValue,
    writer_to_reader: &HashMap<u32, usize>,
) -> Result<(), DeserializeError> {
    let mut pending: Vec<&mut SerialValue> = Vec::from([value]);
    while let Some(value) = pending.pop() {
        match value {
            SerialValue::Index { table, .. } => {
                let reader = writer_to_reader
                    .get(&table.ordinal())
                    .ok_or(DeserializeError::UnknownWriterTable { table: *table })?;
                *table = TableId::new(*reader as u32);
            }
            SerialValue::List(items) => pending.extend(items.iter_mut()),
            SerialValue::Map(entries) => pending.extend(entries.iter_mut().map(|(_, value)| value)),
            SerialValue::Null
            | SerialValue::Bool(_)
            | SerialValue::Int(_)
            | SerialValue::Str(_)
            | SerialValue::Bytes(_) => {}
        }
    }
    Ok(())
}

impl<L: SerializableLang> fmt::Debug for SerdeSession<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct Tables<'a, L: SerializableLang>(&'a [TableState<L>]);
        impl<L: SerializableLang> fmt::Debug for Tables<'_, L> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut list = f.debug_list();
                for table in self.0 {
                    list.entry(&format_args!(
                        "{}: {} entries ({} pending emission)",
                        table.name,
                        table.slots.len(),
                        table.outbox.len()
                    ));
                }
                list.finish()
            }
        }
        f.debug_struct("SerdeSession")
            .field("tables", &Tables(&self.tables))
            .field("user_data", &self.user_data.as_ref().map(|_| "set"))
            .field("descent_guard_init", &self.descent_guard_init)
            .finish()
    }
}
