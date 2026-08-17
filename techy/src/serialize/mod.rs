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
//! [`SerialEntry`]. An *object* is what a table holds — a table entry, referred to
//! by position from wherever it is used; a *value* is data embedded inline in an
//! entry (a state's mode, a source's origin, a span), converted in place and never
//! interned in a table of its own. The *reading environment* is the set of live objects —
//! providers, specs, sources — that the deserializing program already holds and that
//! serialized data can refer to by identity rather than describe in full (handed to
//! the session as its user data); a value that refers to an object the reading
//! environment lacks is a deserialization error.
//!
//! **The capability** is expressed as two pairs of traits. For objects:
//! [`SerializableObject`] — the write side, which every callable spec and provider
//! carries as a supertrait (defaulted, so a type that does not participate writes a
//! one-line empty impl) — and [`DeserializableObject`], the opt-in read side
//! implemented by concrete types only. For values: [`SerializableValue`] and
//! [`DeserializableValue`], implemented by the owner of each value type (the crate
//! covers `()`, `bool`, the integers, `String`, `Option<T>`, `Vec<T>`, and spans;
//! a language covers its own vocabulary and ext types). All four are available only
//! for a language that declares itself serializable by implementing
//! [`SerializableLang`] — whose bounds require the value traits of every type the
//! language supplies to the parse — since their calls receive a [`SerializeContext`]
//! or a [`DeserializeContext`], which exist only for such languages.
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
//! **The standard tables.** [`SerdeSession::new`] registers the drivers of the
//! crate's own object kinds — the sources table ([`SourceSerdeDriver`]), the states
//! table ([`StateSerdeDriver`]), the specs and providers tables
//! ([`SpecSerdeDriver`], [`ProviderSerdeDriver`], dispatching tables whose readers a
//! language or framework registers), and the trees table ([`TreeSerdeDriver`]) — in
//! that order; [`SerdeSession::standard_tables`] returns their handles
//! ([`StandardTables`]), and the extension traits [`StandardTableInterning`],
//! [`StandardTableReading`], and [`TreeSerialization`] intern and read by kind
//! (`cx.intern_source(…)`, `cx.state(…)`, `session.serialize_tree(…)`). A source's
//! text is either *embedded* in its entry or *referenced* — kept outside the
//! serialized form, described by its length and an optional *digest* (a fixed-size
//! fingerprint of the text computed by a hash function the writer chooses, stored as
//! the function's name and output: [`SourceDigest`]); the choice is a caller-supplied
//! [`SourceTextPolicy`], the text and digest verification on reading a caller-supplied
//! [`SourceTextSupplier`], and the crate implements no hash function itself. A state's
//! entry carries its token rules, mode, ext, and scope stack; the derived caches are
//! rebuilt on reading. A tree's entry carries its nodes in storage order (spans, states,
//! specs, and exts referring to the other tables); the reader rebuilds the tree through
//! the node builder, minting a fresh layout tag. Every state and every source is written
//! once however often it is referred to, and read back as one shared object; a tree is
//! a value, written in full on every call.
//!
//! **Specs and providers: identity or a self-contained form.** A callable spec or a
//! provider is serialized either by *identity* — a reference the reading side resolves
//! against the live objects it already holds — or in a *self-contained* form the
//! reading side rebuilds an equivalent object from; each type's own impl decides. A
//! [`Package`](crate::core::specs::Package) is loaded data and goes by identity: its
//! name, resolved through the [`KnownProviders`] directory the reading program sets as
//! the session's user data — the providers it holds, by name, plus
//! [`ProviderRecipe`]s to build the ones it does not hold. A spec that holds parsers
//! (a `StdCallableSpec`, the latexlike macro and environment specs) goes by identity
//! too, through the [`SpecProvenance`](crate::core::specs::SpecProvenance) stamp a
//! package built with [`Package::new_shared`](crate::core::specs::Package::new_shared)
//! hands out — a reference to the package's entry plus the definition key, resolved by
//! looking the key up in the reading side's package of that name: the very instance
//! that package holds, never a lookup re-run (a spec of such a type built outside a
//! shared package cannot be serialized: [`SerializeError::MissingProvenance`]).
//! Scopes and fallback providers are written in full (their definitions as spec
//! positions), the error spec as plain data. The readers of all of these are
//! registered with [`register_core_readers`] — a language's own helper calls it
//! (the latexlike preset's [`latexlike::serialize::register`](crate::latexlike::serialize::register)).
//!
//! **What exists so far.** This module provides the value model, the error types, the
//! capability traits, the engine, the drivers of the sources, states, specs, providers,
//! and trees tables with the crate's own spec and provider serialization, and (with
//! the feature) the rendering layer; the diagnostics driver is not yet present.

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
    register_core_readers, KnownProviders, ProviderIndex, ProviderRecipe, ProviderSerdeDriver,
    ReferencedSource, SourceDigest, SourceIndex, SourceSerdeDriver, SourceTextForm,
    SourceTextPolicy, SourceTextSupplier, SpecIndex, SpecSerdeDriver, StandardTableInterning,
    StandardTableReading, StandardTables, StateIndex, StateSerdeDriver, TreeIndex,
    TreeSerdeDriver, TreeSerialization,
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

// Shared bodies of the crate's own spec serialization, for the preset's impls.
pub(crate) use drivers::specs::{read_unit_recipe, serialize_stamped_spec, unit_recipe_value};

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

// Test helpers reachable in-crate (the tree driver's tests, and the preset's later).
#[cfg(test)]
pub(crate) mod tree_support;
