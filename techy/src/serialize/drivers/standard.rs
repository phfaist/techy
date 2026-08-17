//! The standard tables as a whole: the spec and provider tables' drivers
//! ([`SpecSerdeDriver`], [`ProviderSerdeDriver`]) and positions ([`SpecIndex`],
//! [`ProviderIndex`]), the standard-tables constructor
//! ([`SerdeSession::new`]) with its handle bundle ([`StandardTables`]), and the
//! extension traits [`StandardTableInterning`] and [`StandardTableReading`].

use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;

use crate::scopes::SpecsProvider;
use crate::source::Source;
use crate::spec::CallableSpec;
use crate::state::ParsingState;

use super::super::engine::{
    DeserializeContext, DispatchingSerdeDriver, SerdeSession, SerializeContext, TableHandle,
};
use super::super::error::{DeserializeError, SerializeError};
use super::super::object::SerializableLang;
use super::source::{SourceIndex, SourceSerdeDriver};
use super::state::{StateIndex, StateSerdeDriver};
use super::tree::TreeSerdeDriver;
use super::{PROVIDERS_TABLE, SOURCES_TABLE, SPECS_TABLE, STATES_TABLE, TREES_TABLE};

crate::serial_index! {
    /// A position in the specs table — the `Index` type of [`SpecSerdeDriver`]: the
    /// serialized reference to a callable spec (`Arc<dyn CallableSpec<L>>`).
    pub struct SpecIndex;
}

crate::serial_index! {
    /// A position in the providers table — the `Index` type of
    /// [`ProviderSerdeDriver`]: the serialized reference to a specs provider
    /// (`Arc<dyn SpecsProvider<L>>`).
    pub struct ProviderIndex;
}

/// The driver of the specs table (table name `specs`): the
/// [`DispatchingSerdeDriver`] over `dyn CallableSpec<L>` — every spec type
/// serializes itself through its own
/// [`SerializableObject::serialize_object`](crate::serialize::SerializableObject::serialize_object)
/// (a supertrait of [`CallableSpec`]), and reading dispatches on the entry's
/// identifier through the readers and resolvers registered on the table's handle
/// ([`TableHandle::register_type`] and its siblings). Registered by
/// [`SerdeSession::new`].
pub type SpecSerdeDriver<L> = DispatchingSerdeDriver<L, dyn CallableSpec<L>, SpecIndex>;

/// The driver of the providers table (table name `providers`): the
/// [`DispatchingSerdeDriver`] over `dyn SpecsProvider<L>` — every provider type
/// serializes itself through its own
/// [`SerializableObject::serialize_object`](crate::serialize::SerializableObject::serialize_object)
/// (a supertrait of [`SpecsProvider`]), and reading dispatches on the entry's
/// identifier through the readers and resolvers registered on the table's handle
/// ([`TableHandle::register_type`] and its siblings). Registered by
/// [`SerdeSession::new`].
pub type ProviderSerdeDriver<L> = DispatchingSerdeDriver<L, dyn SpecsProvider<L>, ProviderIndex>;

/// The handles of a session's standard tables, by kind: what
/// [`SerdeSession::standard_tables`] returns for a session that has them (a session
/// built with [`SerdeSession::new`] always does).
#[non_exhaustive]
pub struct StandardTables<L: SerializableLang> {
    /// The sources table.
    pub sources: TableHandle<SourceSerdeDriver<L>>,
    /// The states table.
    pub states: TableHandle<StateSerdeDriver<L>>,
    /// The specs table.
    pub specs: TableHandle<SpecSerdeDriver<L>>,
    /// The providers table.
    pub providers: TableHandle<ProviderSerdeDriver<L>>,
    /// The trees table.
    pub trees: TableHandle<TreeSerdeDriver<L>>,
}

impl<L: SerializableLang> Clone for StandardTables<L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: SerializableLang> Copy for StandardTables<L> {}

impl<L: SerializableLang> fmt::Debug for StandardTables<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StandardTables")
            .field("sources", &self.sources)
            .field("states", &self.states)
            .field("specs", &self.specs)
            .field("providers", &self.providers)
            .field("trees", &self.trees)
            .finish()
    }
}

impl<L: SerializableLang> SerdeSession<L> {
    /// A session with the crate's standard tables registered, in this order: the
    /// sources table ([`SourceSerdeDriver`], with its defaults — every source
    /// embedded, no supplier of referenced source text), the states table
    /// ([`StateSerdeDriver`]), the specs table ([`SpecSerdeDriver`]), the providers
    /// table ([`ProviderSerdeDriver`]), the trees table ([`TreeSerdeDriver`], with the
    /// unit annotation pre-registered). Their handles are
    /// [`standard_tables`](SerdeSession::standard_tables); the interning and reading
    /// accessors by kind are the [`StandardTableInterning`] and
    /// [`StandardTableReading`] extension traits, and, for trees, the
    /// [`TreeSerialization`](crate::serialize::TreeSerialization) extension trait. To
    /// configure the source driver, use
    /// [`with_source_driver`](SerdeSession::with_source_driver); to compose a session
    /// from other tables, [`empty`](SerdeSession::empty).
    ///
    /// The readers of the specs and providers tables (heterogeneous tables) are not
    /// registered here — writing needs none, and a reading session registers them
    /// once: the crate's own with
    /// [`register_core_readers`](crate::serialize::register_core_readers), a language's
    /// with its own helper (which calls the crate's — the latexlike preset's
    /// [`latexlike::serialize::register`](crate::latexlike::serialize::register)), a
    /// framework's own or its resolver on the handles ([`TableHandle::register_type`]
    /// and its siblings). The trees table has the unit annotation registered; other
    /// annotation types are registered with [`TableHandle::register_annotation`].
    pub fn new() -> SerdeSession<L> {
        SerdeSession::with_source_driver(SourceSerdeDriver::new())
    }

    /// [`new`](SerdeSession::new) with the given source driver in place of the
    /// default one — how the source text policy and supplier are configured
    /// ([`SourceSerdeDriver::with_text_policy`], [`SourceSerdeDriver::with_text_supplier`]).
    pub fn with_source_driver(sources: SourceSerdeDriver<L>) -> SerdeSession<L> {
        let mut session = SerdeSession::empty();
        // Invariant: an empty session accepts the five standard tables — their names
        // are distinct, their homogeneous identifiers non-empty, and five tables are
        // far below the table limit — so none of these registrations can fail.
        const ACCEPTED: &str = "an empty session accepts the standard tables";
        session.register_table(sources).expect(ACCEPTED);
        session.register_table(StateSerdeDriver::new()).expect(ACCEPTED);
        session.register_table(SpecSerdeDriver::<L>::new(SPECS_TABLE)).expect(ACCEPTED);
        session.register_table(ProviderSerdeDriver::<L>::new(PROVIDERS_TABLE)).expect(ACCEPTED);
        // The trees table registers the unit annotation itself.
        session.register_table(TreeSerdeDriver::<L>::new()).expect(ACCEPTED);
        session
    }

    /// The handles of the standard tables, if this session has all of them (a session
    /// built with [`new`](SerdeSession::new) or
    /// [`with_source_driver`](SerdeSession::with_source_driver) always does; one
    /// built with [`empty`](SerdeSession::empty) has them when it registered them —
    /// found by name and driver type, see [`table_handle`](SerdeSession::table_handle)).
    pub fn standard_tables(&self) -> Option<StandardTables<L>> {
        Some(StandardTables {
            sources: self.table_handle(SOURCES_TABLE)?,
            states: self.table_handle(STATES_TABLE)?,
            specs: self.table_handle(SPECS_TABLE)?,
            providers: self.table_handle(PROVIDERS_TABLE)?,
            trees: self.table_handle(TREES_TABLE)?,
        })
    }
}

impl<L: SerializableLang> Default for SerdeSession<L> {
    /// [`SerdeSession::new`]: the standard tables.
    fn default() -> Self {
        SerdeSession::new()
    }
}

/// Interning into the standard tables by kind — `intern_source`, `intern_state`,
/// `intern_spec`, `intern_provider` — on a [`SerdeSession`] and, from inside a
/// serialization call, on its [`SerializeContext`]: each finds the standard table by
/// name in the session and interns through it (the general operation is
/// [`SerdeSession::intern`] / [`SerializeContext::intern`] with a table handle).
///
/// An extension trait: bring it into scope with `use techy::serialize::StandardTableInterning;`.
pub trait StandardTableInterning<L: SerializableLang> {
    /// Intern `source` into the sources table (see [`SourceSerdeDriver`]), returning
    /// its position; a source already interned yields its existing position.
    ///
    /// # Errors
    ///
    /// The session has no sources table ([`SerializeError::UnknownTableName`]); the
    /// errors of [`SerdeSession::intern`].
    fn intern_source(&mut self, source: &Arc<Source<L::SourceOrigin>>) -> Result<SourceIndex, SerializeError>;

    /// Intern `state` into the states table (see [`StateSerdeDriver`]), returning its
    /// position; a state already interned yields its existing position.
    ///
    /// # Errors
    ///
    /// The session has no states table ([`SerializeError::UnknownTableName`]); the
    /// errors of [`SerdeSession::intern`].
    fn intern_state(&mut self, state: &Arc<ParsingState<L>>) -> Result<StateIndex, SerializeError>;

    /// Intern `spec` into the specs table (see [`SpecSerdeDriver`]), returning its
    /// position; a spec already interned yields its existing position.
    ///
    /// # Errors
    ///
    /// The session has no specs table ([`SerializeError::UnknownTableName`]); the
    /// spec does not participate in serialization ([`SerializeError::Unsupported`],
    /// wrapped in [`SerializeError::InTable`]); the errors of [`SerdeSession::intern`].
    fn intern_spec(&mut self, spec: &Arc<dyn CallableSpec<L>>) -> Result<SpecIndex, SerializeError>;

    /// Intern `provider` into the providers table (see [`ProviderSerdeDriver`]),
    /// returning its position; a provider already interned yields its existing
    /// position.
    ///
    /// # Errors
    ///
    /// The session has no providers table ([`SerializeError::UnknownTableName`]);
    /// the provider does not participate in serialization
    /// ([`SerializeError::Unsupported`], wrapped in [`SerializeError::InTable`]); the
    /// errors of [`SerdeSession::intern`].
    fn intern_provider(&mut self, provider: &Arc<dyn SpecsProvider<L>>) -> Result<ProviderIndex, SerializeError>;
}

/// Reading objects back from the standard tables by kind — `source`, `state`,
/// `spec`, `provider` — on a [`SerdeSession`] and, from inside a deserialization
/// call, on its [`DeserializeContext`]: each finds the standard table by name in the
/// session and reads through it (the general operation is [`SerdeSession::object`] /
/// [`DeserializeContext::object`] with a table handle). The same position always
/// yields the same `Arc`.
///
/// An extension trait: bring it into scope with `use techy::serialize::StandardTableReading;`.
pub trait StandardTableReading<L: SerializableLang> {
    /// The source at `position` of the sources table.
    ///
    /// # Errors
    ///
    /// The session has no sources table ([`DeserializeError::UnknownTableName`]);
    /// the errors of [`SerdeSession::object`].
    fn source(&mut self, position: SourceIndex) -> Result<Arc<Source<L::SourceOrigin>>, DeserializeError>;

    /// The state at `position` of the states table.
    ///
    /// # Errors
    ///
    /// The session has no states table ([`DeserializeError::UnknownTableName`]);
    /// the errors of [`SerdeSession::object`].
    fn state(&mut self, position: StateIndex) -> Result<Arc<ParsingState<L>>, DeserializeError>;

    /// The spec at `position` of the specs table.
    ///
    /// # Errors
    ///
    /// The session has no specs table ([`DeserializeError::UnknownTableName`]); the
    /// errors of [`SerdeSession::object`].
    fn spec(&mut self, position: SpecIndex) -> Result<Arc<dyn CallableSpec<L>>, DeserializeError>;

    /// The provider at `position` of the providers table.
    ///
    /// # Errors
    ///
    /// The session has no providers table ([`DeserializeError::UnknownTableName`]);
    /// the errors of [`SerdeSession::object`].
    fn provider(&mut self, position: ProviderIndex) -> Result<Arc<dyn SpecsProvider<L>>, DeserializeError>;
}

fn no_table_to_write(name: &str) -> SerializeError {
    SerializeError::UnknownTableName { name: String::from(name) }
}

fn no_table_to_read(name: &str) -> DeserializeError {
    DeserializeError::UnknownTableName { name: String::from(name) }
}

/// The two impls of [`StandardTableInterning`], for the session and the context —
/// which share the `table_handle` / `intern` surface.
macro_rules! standard_table_interning {
    ($($host:tt)*) => {
        impl<L: SerializableLang> StandardTableInterning<L> for $($host)* {
            fn intern_source(
                &mut self,
                source: &Arc<Source<L::SourceOrigin>>,
            ) -> Result<SourceIndex, SerializeError> {
                let table = self.table_handle::<SourceSerdeDriver<L>>(SOURCES_TABLE)
                    .ok_or_else(|| no_table_to_write(SOURCES_TABLE))?;
                self.intern(table, source)
            }

            fn intern_state(&mut self, state: &Arc<ParsingState<L>>) -> Result<StateIndex, SerializeError> {
                let table = self.table_handle::<StateSerdeDriver<L>>(STATES_TABLE)
                    .ok_or_else(|| no_table_to_write(STATES_TABLE))?;
                self.intern(table, state)
            }

            fn intern_spec(&mut self, spec: &Arc<dyn CallableSpec<L>>) -> Result<SpecIndex, SerializeError> {
                let table = self.table_handle::<SpecSerdeDriver<L>>(SPECS_TABLE)
                    .ok_or_else(|| no_table_to_write(SPECS_TABLE))?;
                self.intern(table, spec)
            }

            fn intern_provider(
                &mut self,
                provider: &Arc<dyn SpecsProvider<L>>,
            ) -> Result<ProviderIndex, SerializeError> {
                let table = self.table_handle::<ProviderSerdeDriver<L>>(PROVIDERS_TABLE)
                    .ok_or_else(|| no_table_to_write(PROVIDERS_TABLE))?;
                self.intern(table, provider)
            }
        }
    };
}

standard_table_interning!(SerdeSession<L>);
standard_table_interning!(SerializeContext<'_, L>);

/// The two impls of [`StandardTableReading`], for the session and the context —
/// which share the `table_handle` / `object` surface.
macro_rules! standard_table_reading {
    ($($host:tt)*) => {
        impl<L: SerializableLang> StandardTableReading<L> for $($host)* {
            fn source(
                &mut self,
                position: SourceIndex,
            ) -> Result<Arc<Source<L::SourceOrigin>>, DeserializeError> {
                let table = self.table_handle::<SourceSerdeDriver<L>>(SOURCES_TABLE)
                    .ok_or_else(|| no_table_to_read(SOURCES_TABLE))?;
                self.object(table, position)
            }

            fn state(&mut self, position: StateIndex) -> Result<Arc<ParsingState<L>>, DeserializeError> {
                let table = self.table_handle::<StateSerdeDriver<L>>(STATES_TABLE)
                    .ok_or_else(|| no_table_to_read(STATES_TABLE))?;
                self.object(table, position)
            }

            fn spec(&mut self, position: SpecIndex) -> Result<Arc<dyn CallableSpec<L>>, DeserializeError> {
                let table = self.table_handle::<SpecSerdeDriver<L>>(SPECS_TABLE)
                    .ok_or_else(|| no_table_to_read(SPECS_TABLE))?;
                self.object(table, position)
            }

            fn provider(
                &mut self,
                position: ProviderIndex,
            ) -> Result<Arc<dyn SpecsProvider<L>>, DeserializeError> {
                let table = self.table_handle::<ProviderSerdeDriver<L>>(PROVIDERS_TABLE)
                    .ok_or_else(|| no_table_to_read(PROVIDERS_TABLE))?;
                self.object(table, position)
            }
        }
    };
}

standard_table_reading!(SerdeSession<L>);
standard_table_reading!(DeserializeContext<'_, L>);
