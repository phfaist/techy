//! The drivers of the crate's own standard tables and the accessors around them: the
//! source driver ([`SourceSerdeDriver`], with the embed-or-reference choice and the
//! digest contract), the state driver ([`StateSerdeDriver`]), the two dispatching
//! drivers of the spec and provider tables ([`SpecSerdeDriver`],
//! [`ProviderSerdeDriver`]), their typed positions ([`SourceIndex`], [`StateIndex`],
//! [`SpecIndex`], [`ProviderIndex`]), the standard-tables constructor
//! ([`SerdeSession::new`](crate::serialize::SerdeSession::new)) with its handle bundle
//! ([`StandardTables`]), the extension traits that intern into and read from
//! the standard tables by kind ([`StandardTableInterning`], [`StandardTableReading`]),
//! the tree driver ([`TreeSerdeDriver`]), and the serialization of the crate's own
//! spec and provider types with the reading environment's provider directory
//! ([`KnownProviders`], [`register_core_readers`]).
//!
//! Everything here is registered on the type-blind engine exactly as a framework's
//! own tables would be: the drivers implement
//! [`ObjectSerdeDriver`](crate::serialize::ObjectSerdeDriver), the positions are
//! [`serial_index!`](crate::serialize::serial_index) types, and the accessors find
//! the tables by name ([`SerdeSession::table_handle`](crate::serialize::SerdeSession::table_handle)).

mod source;
pub(crate) mod specs;
mod standard;
mod state;
mod tree;

pub use source::{
    ReferencedSource, SourceDigest, SourceIndex, SourceSerdeDriver, SourceTextForm,
    SourceTextPolicy, SourceTextSupplier,
};
pub use specs::{register_core_readers, KnownProviders, ProviderRecipe};
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

/// The names of the standard tables, in registration order.
pub(crate) const STANDARD_TABLE_NAMES: &[&str] =
    &[SOURCES_TABLE, STATES_TABLE, SPECS_TABLE, PROVIDERS_TABLE, TREES_TABLE];

/// The name of the first standard table `session` lacks (by name), or the string
/// "standard tables" when every name is registered but not with the standard
/// driver type — what a registration helper names when
/// [`SerdeSession::standard_tables`](crate::serialize::SerdeSession::standard_tables)
/// answers `None`.
pub(crate) fn missing_standard_table<L: crate::serialize::SerializableLang>(
    session: &crate::serialize::SerdeSession<L>,
) -> &'static str {
    STANDARD_TABLE_NAMES
        .iter()
        .copied()
        .find(|name| session.table_ordinal_by_name(name).is_none())
        .unwrap_or("standard tables")
}

/// The identifier of every entry of the sources table.
pub(crate) const SOURCE_IDENTIFIER: &str = "core.source";
/// The identifier of every entry of the states table.
pub(crate) const STATE_IDENTIFIER: &str = "core.state";
/// The identifier of a trees table entry whose annotation is the unit type (the
/// annotations are omitted from the wire).
pub(crate) const CORE_TREE_IDENTIFIER: &str = "core.tree";
/// The identifier of a specs table entry holding a stamped spec's identity form
/// (its provider's position plus the definition key).
pub(crate) const SPEC_IDENTITY_IDENTIFIER: &str = "core.provider-spec";
/// The identifier of a specs table entry holding an error spec's self-contained form.
pub(crate) const ERROR_SPEC_IDENTIFIER: &str = "core.error-spec";
/// The identifier of a providers table entry holding a package (by name).
pub(crate) const PACKAGE_IDENTIFIER: &str = "core.package";
/// The identifier of a providers table entry holding a scope (in full).
pub(crate) const SCOPE_IDENTIFIER: &str = "core.scope";
/// The identifier of a providers table entry holding a fallback provider (in full).
pub(crate) const FALLBACK_PROVIDER_IDENTIFIER: &str = "core.fallback-provider";

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tree_tests;
