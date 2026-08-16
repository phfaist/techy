//! Serialization: converting the objects techy consumers handle — parsed node trees,
//! parsing states, sources, callable specs and providers, diagnostics — to and from a
//! format-independent, in-memory value model, so they can be stored or transmitted.
//!
//! **Vocabulary used throughout this module.** *Serialization* converts a live object
//! into a [`SerialValue`], a plain tree of values (booleans, integers, strings, byte
//! strings, lists, string-keyed maps, table references) that depends on no encoding
//! format; *deserialization* is the reverse. Objects that are shared — one parsing
//! state referenced by many nodes, one source referenced by many spans — are written
//! once into a numbered *table* and referred to by their position in it, so sharing
//! survives the round trip; a [`TableId`] names a table, and the position within it
//! travels as a [`SerialValue::Index`]. Every serialized object carries an
//! *identifier*: a deliberately chosen, stable string naming what kind of object the
//! value describes (never a Rust type name), returned as part of a [`SerialEntry`].
//! The *reading environment* is the set of live objects — providers, specs, sources —
//! that the deserializing program already holds and that serialized data can refer
//! to by identity rather than describe in full; a value that refers to an object the
//! reading environment lacks is a deserialization error.
//!
//! The capability is expressed as two traits: [`SerializableObject`] — the write
//! side, which every callable spec and provider carries as a supertrait (defaulted, so
//! a type that does not participate writes a one-line empty impl) — and
//! [`DeserializableObject`], the opt-in read side implemented by concrete types only.
//! Both are available only for a language that declares itself serializable by
//! implementing [`SerializableLang`]: their calls receive a [`SerializeContext`] or a
//! [`DeserializeContext`], which exist only for such languages.
//!
//! **Cargo features.** Everything in this module is unconditional plain Rust with no
//! external dependency (`no_std` + `alloc`). The optional `serde` cargo feature adds
//! the rendering layer: `Serialize`/`Deserialize` impls for [`SerialValue`], which
//! encode a value through any serde format (see the type's documentation for the
//! rendering), and the bridge — `to_value` / `from_value` — which converts any type
//! implementing serde's traits to and from a `SerialValue`, enforcing the value
//! model's rules ([`SerialValueError`]); `serial_bytes` marks a byte-string field for
//! it. The feature adds no obligation to any implementer of the traits here.
//!
//! **What exists so far.** This module currently provides the value model, the error
//! types, the capability traits, and (with the feature) the rendering layer; the
//! machinery that walks whole trees and manages the tables is not yet present, and
//! the contexts have no public operations yet.

mod engine;
mod error;
mod object;
mod value;
pub(crate) mod wire;

#[cfg(feature = "serde")]
mod base64;
#[cfg(feature = "serde")]
mod bridge;
#[cfg(feature = "serde")]
mod render;

pub use engine::{DeserializeContext, SerializeContext};
pub use error::{DeserializeError, SerialValueError, SerializeError};
pub use object::{DeserializableObject, SerializableLang, SerializableObject};
pub use value::{SerialEntry, SerialIndex, SerialValue, TableId};

#[cfg(feature = "serde")]
pub use bridge::{from_value, serial_bytes, to_value};

#[cfg(test)]
mod tests;
#[cfg(all(test, feature = "serde"))]
mod serde_tests;
