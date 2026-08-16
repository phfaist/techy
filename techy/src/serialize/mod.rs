//! Serialization: converting the objects techy consumers handle — parsed node trees,
//! parsing states, sources, callable specs and providers, diagnostics — to and from a
//! format-independent, in-memory value model, so they can be stored or transmitted.
//!
//! **Vocabulary used throughout this module.** *Serialization* converts a live object
//! into a [`SerialValue`], a plain tree of values (booleans, integers, strings, byte
//! strings, lists, string-keyed maps, table references) that depends on no encoding
//! format; *deserialization* is the reverse. The *wire* is the serialized form
//! itself — the data as it is written out and read back, as opposed to the live
//! objects it describes; a "wire struct" or "wire name" is a structure or a key name
//! of that form. Objects are written into *tables* — one table per kind of object,
//! kept by a [`SerdeSession`] and numbered in registration order ([`TableId`]) —
//! and referred to by their position in the table; a table is *homogeneous* when it
//! holds objects of one kind only (every entry has the same identifier, which the
//! table then does not write out) and *heterogeneous* when it holds trait objects of
//! several concrete types (every entry carries its own identifier). *Interning* an
//! object writes it into its table once and returns its position, so that an object
//! referred to from several places (one parsing state referenced by many nodes, one
//! source referenced by many spans) is written once and the sharing survives the
//! round trip; *reading an object back* ([`SerdeSession::object`],
//! [`DeserializeContext::object`]) returns the object stored at a position,
//! rebuilding it from its wire entry on first use, so that the same position always
//! yields the same object. A position travels as a [`SerialValue::Index`] and, in
//! Rust code, as a typed position type (a [`SerialIndex`] implementer, defined with
//! [`serial_index!`]). A *segment* ([`Segment`]) is the unit a session emits and
//! absorbs: the entries new since the previous emission, table by table; a *stream*
//! is the sequence of segments one session emits. Positions are scoped to the
//! stream — later segments refer to earlier ones' entries by position — so a reading
//! session absorbs the segments of one stream only, in order; and a typed position
//! held in Rust code is scoped further, to the session holding it (it carries that
//! session's [`TableId`]; between sessions a position is exchanged as its table's
//! name and its `u32` index — see [`SerialIndex`]). Every serialized object carries
//! an *identifier*: a deliberately chosen, stable string naming what kind of object
//! the value describes (never a Rust type name), returned as part of a
//! [`SerialEntry`]. The *reading environment* is the set of live objects —
//! providers, specs, sources — that the deserializing program already holds and that
//! serialized data can refer to by identity rather than describe in full (handed to
//! the session as its user data); a value that refers to an object the reading
//! environment lacks is a deserialization error.
//!
//! **The capability** is expressed as two traits: [`SerializableObject`] — the write
//! side, which every callable spec and provider carries as a supertrait (defaulted, so
//! a type that does not participate writes a one-line empty impl) — and
//! [`DeserializableObject`], the opt-in read side implemented by concrete types only.
//! Both are available only for a language that declares itself serializable by
//! implementing [`SerializableLang`]: their calls receive a [`SerializeContext`] or a
//! [`DeserializeContext`], which exist only for such languages.
//!
//! **The engine** is a [`SerdeSession`]: it holds the tables, each registered with
//! an [`ObjectSerdeDriver`] (how the objects of that table are serialized and
//! rebuilt) and addressed through its typed [`TableHandle`]; interns objects
//! ([`SerdeSession::intern`], [`SerializeContext::intern`]) and reads objects back
//! from positions ([`SerdeSession::object`], [`DeserializeContext::object`]);
//! emits and absorbs segments ([`SerdeSession::take_segment`],
//! [`SerdeSession::push_segment`]). A table of trait objects of several concrete
//! types uses the [`DispatchingSerdeDriver`], whose reading side dispatches on the
//! entry's identifier through registered [`ObjectReader`]s and
//! [`IdentifierResolver`]s. Reads treat everything as untrusted input: a malformed
//! segment, a reference out of range or into the wrong table, a reference cycle, or
//! an unknown identifier is an error naming the culprit — never a panic — and a
//! failed absorption leaves the session as it was.
//!
//! **Cargo features.** Everything in this module is unconditional plain Rust with no
//! external dependency (`no_std` + `alloc`): sessions produce and absorb in-memory
//! segments without any feature. The optional `serde` cargo feature adds the
//! rendering layer: `Serialize`/`Deserialize` impls for [`SerialValue`] and
//! [`Segment`], which encode through any serde format (see the types' documentation
//! for the rendering), and the bridge — `to_value` / `from_value` — which converts any
//! type implementing serde's traits to and from a `SerialValue`, enforcing the value
//! model's rules ([`SerialValueError`]); `serial_bytes` marks a byte-string field for
//! it. The feature adds no obligation to any implementer of the traits here.
//!
//! **What exists so far.** This module provides the value model, the error types, the
//! capability traits, the engine, and (with the feature) the rendering layer; the
//! drivers for the crate's own object kinds — sources, states, trees, specs,
//! providers, diagnostics — are not yet present, so a session currently holds only
//! the tables its user registers.

mod drivers;
mod engine;
mod error;
mod object;
mod serial_index;
mod value;
pub(crate) mod wire;

#[cfg(feature = "serde")]
mod base64;
#[cfg(feature = "serde")]
pub(crate) mod bridge;
#[cfg(feature = "serde")]
mod render;

pub use drivers::{
    ProviderIndex, ProviderSerdeDriver, ReferencedSource, SourceDigest, SourceIndex,
    SourceSerdeDriver, SourceTextForm, SourceTextPolicy, SourceTextSupplier, SpecIndex,
    SpecSerdeDriver, StandardTableInterning, StandardTableReading, StandardTables, StateIndex,
    StateSerdeDriver,
};
pub use engine::{
    DeserializeContext, DispatchingSerdeDriver, IdentifierResolver, ObjectReader,
    ObjectSerdeDriver, SerdeSession, Segment, SegmentTable, SerializeContext, TableHandle,
};
pub use error::{DeserializeError, RegistrationError, SerialValueError, SerializeError};
pub use object::{
    DeserializableObject, DeserializableValue, SerializableLang, SerializableObject,
    SerializableValue,
};
pub use value::{SerialEntry, SerialIndex, SerialValue, TableId};

// The typed-position macro is defined at the crate root (as every `macro_rules!`
// export is) and hidden there; this is its canonical, documented path.
#[doc(inline)]
pub use crate::serial_index;

#[cfg(feature = "serde")]
pub use bridge::{from_value, serial_bytes, to_value};

#[cfg(test)]
mod tests;
#[cfg(all(test, feature = "serde"))]
mod serde_tests;
