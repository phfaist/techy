//! The serialization engine: the session and its tables ([`SerdeSession`],
//! [`TableHandle`]), the per-table driver ([`ObjectSerdeDriver`]), the contexts handed
//! to serialization and deserialization calls ([`SerializeContext`],
//! [`DeserializeContext`]), the read dispatch of heterogeneous tables
//! ([`DispatchingSerdeDriver`], [`ObjectReader`], [`IdentifierResolver`]), and the
//! unit of emission ([`Segment`], [`SegmentTable`]). Type-blind: nothing here names a
//! source, a state, or a spec — every object kind is registered on it identically.

mod context;
mod dispatch;
mod driver;
mod segment;
mod session;

pub use context::{DeserializeContext, SerializeContext};
pub use dispatch::{DispatchingSerdeDriver, IdentifierResolver, ObjectReader};
pub use driver::{ObjectSerdeDriver, TableHandle};
pub use segment::{Segment, SegmentTable};
pub use session::SerdeSession;

// Crate-internal: the per-table registry trait a custom driver (the trees table)
// keeps its own registrations behind.
pub(crate) use session::TableRegistry;

#[cfg(test)]
mod tests;
