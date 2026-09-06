//! The serialization engine: the session, its tables, and the segments they exchange.
//!
//! [`SerdeSession`] holds the tables and is the entry point; [`TableHandle`] is a typed
//! handle on one of them, and [`ObjectSerdeDriver`] says how the objects of one table
//! are serialized and rebuilt. [`SerializeContext`] and [`DeserializeContext`] are what
//! a driver's calls receive to reach the session. [`DispatchingSerdeDriver`], with
//! [`ObjectReader`] and [`IdentifierResolver`], is the driver of a table holding objects
//! of several concrete types. [`Segment`] and [`SegmentTable`] are what a session emits
//! and absorbs.
//!
//! Nothing here names a source, a state, or a spec: every object kind is registered on
//! the engine the same way, and the crate's own kinds are registered exactly as a
//! framework's own would be.

mod context;
mod dispatch;
mod driver;
mod segment;
mod session;

pub use context::{DeserializeContext, SerializeContext};
pub use dispatch::{DispatchingSerdeDriver, IdentifierResolver, ObjectReader};
pub use driver::{ObjectSerdeDriver, TableHandle};
pub use segment::{Segment, SegmentMeta, SegmentTable};
pub use session::SerdeSession;

// Crate-internal: the per-table registry trait a custom driver (the trees table)
// keeps its own registrations behind.
pub(crate) use session::TableRegistry;
// The greatest nesting depth of a stored entry (the tests pin it).
#[cfg(test)]
pub(crate) use segment::MAX_ENTRY_NESTING_DEPTH;

#[cfg(test)]
mod tests;
