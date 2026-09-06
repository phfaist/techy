//! The drivers of the crate's own standard tables, and the accessors around them.
//!
//! There is one driver per kind of object: [`SourceSerdeDriver`] for sources (with the
//! choice between embedding a source's text and referencing it), [`StateSerdeDriver`]
//! for parsing states, [`SpecSerdeDriver`] and [`ProviderSerdeDriver`] for callable
//! specs and specs providers, [`TreeSerdeDriver`] for node trees,
//! [`DiagnosticSerdeDriver`] for diagnostics, and [`ParseResultSerdeDriver`] for whole
//! parse results. Each table has its own typed position type ([`SourceIndex`],
//! [`StateIndex`], and so on).
//!
//! [`SerdeSession::new`](crate::serialize::SerdeSession::new) registers all seven and
//! reports their handles as [`StandardTables`]. Objects go in and come out either
//! through those handles or through the by-kind extension traits:
//! [`StandardTableInterning`] and [`StandardTableReading`] for sources, states, specs,
//! and providers, and [`TreeSerialization`], [`DiagnosticSerialization`], and
//! [`ParseResultSerialization`] for trees, diagnostics, and parse results.
//!
//! Reading also needs the reading environment: [`KnownProviders`] is the directory of
//! providers a serialized package resolves against, and [`register_core_readers`]
//! registers the readers of the crate's own spec and provider types.
//!
//! Everything here is registered on the engine, which knows nothing of these object
//! types, exactly as a framework's own tables would be: the drivers implement
//! [`ObjectSerdeDriver`](crate::serialize::ObjectSerdeDriver), the positions are
//! [`serial_index!`](crate::serialize::serial_index) types, and the accessors find
//! the tables by name ([`SerdeSession::table_handle`](crate::serialize::SerdeSession::table_handle)).

mod diagnostic;
mod parse_result;
mod source;
pub(crate) mod specs;
mod standard;
mod state;
mod tree;

pub use diagnostic::{
    DeserializedCondition, DiagnosticIndex, DiagnosticSerdeDriver, DiagnosticSerialization,
};
pub use parse_result::{ParseResultIndex, ParseResultSerdeDriver, ParseResultSerialization};
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
/// The name of the diagnostics table.
pub(crate) const DIAGNOSTICS_TABLE: &str = "diagnostics";
/// The name of the parse-results table.
pub(crate) const PARSE_RESULTS_TABLE: &str = "parse-results";

/// The identifier of every entry of the sources table.
pub(crate) const SOURCE_IDENTIFIER: &str = "core.source";
/// The identifier of every entry of the states table.
pub(crate) const STATE_IDENTIFIER: &str = "core.state";
/// The identifier of a trees table entry whose annotation is the unit type (the
/// annotations are omitted from the wire).
pub(crate) const CORE_TREE_IDENTIFIER: &str = "core.tree";
/// The identifier of every entry of the diagnostics table.
pub(crate) const DIAGNOSTIC_IDENTIFIER: &str = "core.diagnostic";
/// The identifier of every entry of the parse-results table.
pub(crate) const PARSE_RESULT_IDENTIFIER: &str = "core.parse-result";
/// The identifier of a specs table entry holding a stamped spec's identity form
/// (its provider's position plus the definition key).
pub(crate) const SPEC_IDENTITY_IDENTIFIER: &str = "core.provider-spec-identity";
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
#[cfg(test)]
mod diagnostic_tests;
