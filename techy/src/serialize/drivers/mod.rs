//! The drivers of the crate's own standard tables and the accessors around them: the
//! source driver ([`SourceSerdeDriver`], with the embed-or-reference choice and the
//! digest contract), the state driver ([`StateSerdeDriver`]), the two dispatching
//! drivers of the spec and provider tables ([`SpecSerdeDriver`],
//! [`ProviderSerdeDriver`]), their typed positions ([`SourceIndex`], [`StateIndex`],
//! [`SpecIndex`], [`ProviderIndex`]), the standard-tables constructor
//! ([`SerdeSession::new`](crate::serialize::SerdeSession::new)) with its handle bundle
//! ([`StandardTables`]), and the extension traits that intern into and read from
//! the standard tables by kind ([`StandardTableInterning`], [`StandardTableReading`]).
//!
//! Everything here is registered on the type-blind engine exactly as a framework's
//! own tables would be: the drivers implement
//! [`ObjectSerdeDriver`](crate::serialize::ObjectSerdeDriver), the positions are
//! [`serial_index!`](crate::serialize::serial_index) types, and the accessors find
//! the tables by name ([`SerdeSession::table_handle`](crate::serialize::SerdeSession::table_handle)).

mod source;
mod standard;
mod state;
mod tree;

pub use source::{
    ReferencedSource, SourceDigest, SourceIndex, SourceSerdeDriver, SourceTextForm,
    SourceTextPolicy, SourceTextSupplier,
};
pub use standard::{
    ProviderIndex, ProviderSerdeDriver, SpecIndex, SpecSerdeDriver, StandardTableInterning,
    StandardTableReading, StandardTables,
};
pub use state::{StateIndex, StateSerdeDriver};
pub use tree::{TreeIndex, TreeSerdeDriver, TreeSerialization};

/// The name of the sources table.
pub(crate) const SOURCES_TABLE: &str = "sources";
/// The name of the states table.
pub(crate) const STATES_TABLE: &str = "states";
/// The name of the specs table.
pub(crate) const SPECS_TABLE: &str = "specs";
/// The name of the providers table.
pub(crate) const PROVIDERS_TABLE: &str = "providers";
/// The name of the trees table.
pub(crate) const TREES_TABLE: &str = "trees";

/// The identifier of every entry of the sources table.
pub(crate) const SOURCE_IDENTIFIER: &str = "core.source";
/// The identifier of every entry of the states table.
pub(crate) const STATE_IDENTIFIER: &str = "core.state";
/// The identifier of a trees table entry whose annotation is the unit type (the
/// annotations are omitted from the wire).
pub(crate) const CORE_TREE_IDENTIFIER: &str = "core.tree";

#[cfg(test)]
mod tests;
