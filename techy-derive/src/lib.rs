//! Derive macros for techy: the declaration of structured diagnostic conditions, and
//! the serialization of a language's own value types.
//!
//! This is the build-time companion crate of `techy`. `techy` re-exports every public
//! derive next to the trait it implements — depend on `techy` and use the re-exports
//! rather than depending on this crate directly.
//!
//! From `techy::error`:
//!
//! - [`DiagnosticInfo`](macro@DiagnosticInfo) — on a condition data struct: generates
//!   the `DiagnosticInfo` impl (`IDENTIFIER`, `serializable_data()`), optionally a
//!   `Display` impl from a message format string, and the `new()` constructor.
//! - [`ToDiagnosticValue`](macro@ToDiagnosticValue) — on a field-less payload enum:
//!   serializes as the kebab-cased variant name.
//!
//! From `techy::serialize`:
//!
//! - [`SerializableValue`](macro@SerializableValue) /
//!   [`DeserializableValue`](macro@DeserializableValue) — on a plain-data struct or
//!   enum whose every field and variant carries `#[serial(name = "…")]`: the two value
//!   conversion impls (`SerializableValue<L>` / `DeserializableValue<L>`), for every
//!   language — or for the one language named by `#[serial(lang = …)]` on the type —
//!   each field converting through its own impl with the serialization context.
//!
//! The crate also carries a techy-internal pair (`ToSerialValue` / `FromSerialValue`,
//! hidden) whose generated code compiles only inside techy itself.

mod diagnostic_info;
mod serial_value;
mod to_value;
mod value_derive;

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

/// Derives `techy::error::DiagnosticInfo` for a condition data struct.
///
/// ```ignore
/// #[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
/// #[non_exhaustive]
/// #[diagnostic(
///     id = "core.specs.unresolvable-command",
///     message = "cannot resolve command ‘{escape_char}{name}’"
/// )]
/// pub struct UnresolvableCommand {
///     /// The command name, as written (without the escape character).
///     pub name: String,
///     /// The escape character that introduced the command.
///     pub escape_char: char,
/// }
/// ```
///
/// Generated:
///
/// - the `DiagnosticInfo` impl — `IDENTIFIER` from the **mandatory** `id` (the wire
///   identity is hand-chosen, never derived from the type name), and
///   `serializable_data()` mapping every field through `techy::error::ToDiagnosticValue`,
///   keyed by field name — or by the field's `#[diagnostic(key = "…")]` when the
///   serialization key must differ from the Rust name (`ch` → `"char"`, say; the
///   key is part of the stable wire contract, the field name is not). A field whose
///   type does not implement `ToDiagnosticValue`
///   fails with an error at the field — serializability of the payload is enforced by
///   the compiler.
/// - a `Display` impl from the **optional** `message` format string; `{field}` (with
///   optional format spec, `{field:04}`) interpolates fields. Omit `message` and write
///   `Display` by hand when the wording needs a match, a conditional, or a cast.
/// - the `new()` constructor with `impl Into<FieldType>` parameters — the companion of
///   `#[non_exhaustive]`. Opt out with `no_constructor` for a bespoke signature.
#[proc_macro_derive(DiagnosticInfo, attributes(diagnostic))]
pub fn derive_diagnostic_info(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    diagnostic_info::expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives `techy::error::ToDiagnosticValue` for a field-less enum: the value
/// serializes as `DiagnosticValue::Str` of the kebab-cased variant name
/// (`EndOfInput` → `"end-of-input"`).
///
/// ```ignore
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, ToDiagnosticValue)]
/// #[non_exhaustive]
/// pub enum MissingTerminatorFound {
///     EndOfInput,
///     StrayGroupClose,
/// }
/// ```
///
/// Variants carrying data, and non-enum types, need a hand-written impl instead
/// (condition structs get their serialization from `#[derive(DiagnosticInfo)]`).
#[proc_macro_derive(ToDiagnosticValue)]
pub fn derive_to_diagnostic_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    to_value::expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives `techy::serialize::SerializableValue` for a plain-data struct or enum: the
/// write side of the value conversion, for every language — or for the one language
/// named by `#[serial(lang = …)]` on the type.
///
/// ```ignore
/// use techy::serialize::{DeserializableValue, SerializableValue};
///
/// #[derive(SerializableValue, DeserializableValue)]
/// struct Placement {
///     #[serial(name = "kind")]
///     kind: Kind,                // converts through its own (derived) impl
///     #[serial(name = "width")]
///     width: u32,
///     #[serial(name = "note")]
///     note: Option<String>,      // left out of the map when `None`
/// }
///
/// #[derive(SerializableValue, DeserializableValue)]
/// enum Kind {
///     #[serial(name = "plain")]
///     Plain,
///     #[serial(name = "named")]
///     Named(String),
///     #[serial(name = "sized")]
///     Sized {
///         #[serial(name = "width")]
///         width: u32,
///     },
/// }
/// ```
///
/// Generated: `impl<L: Lang> SerializableValue<L> for Placement`, whose
/// `serialize_value` converts every field through the field type's own
/// `SerializableValue<L>` impl, passing the serialization context along, and
/// assembles the serialized form:
///
/// - a struct is a map from each field's wire name to the field's value, in
///   declaration order; a field whose value is absent (an `Option` that is `None`) is
///   left out of the map — no key, rather than a key with `null`;
/// - a unit enum variant is the string of its wire name (`"plain"`); a newtype
///   variant is a one-entry map from the wire name to the payload's value
///   (`{"named": "x"}`); a variant with named fields is a one-entry map from the
///   wire name to the field map (`{"sized": {"width": 3}}`).
///
/// This is the serialized form techy's own structures use, and the one the `serde`
/// bridge produces for the corresponding serde shapes (with `rename`,
/// `skip_serializing_if = "Option::is_none"`, `default`, and
/// `deny_unknown_fields`).
///
/// **Wire names.** Every field and every variant carries `#[serial(name = "…")]`; a
/// field or variant without one is a compile error. Wire names are part of what a
/// program writes and later reads back, possibly across versions of the program: they
/// are chosen deliberately and never taken from Rust identifiers, which are renamed
/// freely. The only `serial` attribute the type itself may carry is `lang`, below; it
/// must name a language that implements `SerializableLang` (the context types in the
/// generated signatures require it — otherwise the derive fails with an unsatisfied
/// bound).
///
/// **The language.** Without a type-level attribute the impl is for every language
/// (`impl<L: Lang> SerializableValue<L> for T`), so every field type must implement
/// `SerializableValue<L>` for every `L: Lang` — as the crate's impls for `bool`, the
/// integers, `char`, `String`, `Option<T>`, `Vec<T>`, and `SerialValue` do, and every
/// type derived this way; a field type without the impl is a compile error at the
/// field. A field whose conversion exists for one language only needs that language
/// named on the type: `#[serial(lang = MyLang)]` generates
/// `impl SerializableValue<MyLang> for T` instead, and the type is then usable by that
/// language only. The common case is a span — `SourceSpan<O>` converts for the
/// languages whose `SourceOrigin` is `O`, and interns its source through the context
/// exactly as in a hand-written impl:
///
/// ```ignore
/// use techy::core::TrivialLang;
/// use techy::serialize::{DeserializableValue, SerializableLang, SerializableValue};
/// use techy::source::SourceSpan;
///
/// #[derive(Debug, Clone, Copy)]
/// struct MyLang;
/// impl TrivialLang for MyLang {}       // its `SourceOrigin` is `Option<String>`
/// impl SerializableLang for MyLang {}
///
/// #[derive(SerializableValue, DeserializableValue)]
/// #[serial(lang = MyLang)]
/// struct Located {
///     #[serial(name = "span")]
///     span: SourceSpan<Option<String>>,   // interns its source through the context
///     #[serial(name = "kind")]
///     kind: Kind,
/// }
/// ```
///
/// **Supported shapes.** Structs with named fields; enums with at least one variant,
/// each a unit variant, a newtype variant (one unnamed field), or a variant with named
/// fields. No generic types: a value whose type depends on the language cannot be a
/// field of a type derived for every language — hold it as an already converted
/// `SerialValue`, or name the language with `#[serial(lang = …)]`.
///
/// **When to write the impl by hand.** When the serialized form must differ from the
/// type's own layout — a value computed from the type, a field holding a shared
/// handle, a form kept stable across a change of the type — write `serialize_value`
/// by hand; where useful, convert through a separate struct that has the serialized
/// layout and derives this trait.
#[proc_macro_derive(SerializableValue, attributes(serial))]
pub fn derive_serializable_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    value_derive::expand_serializable_value(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives `techy::serialize::DeserializableValue` for a plain-data struct or enum —
/// the read direction of [`SerializableValue`](macro@SerializableValue): the same
/// attributes and shapes, reading the serialized form that derive writes.
///
/// Generated: `impl<L: Lang> DeserializableValue<L> for T` — or, with
/// `#[serial(lang = MyLang)]` on the type, `impl DeserializableValue<MyLang> for T` —
/// whose `deserialize_value` reads every field through the field type's own
/// `DeserializableValue<L>` impl with the deserialization context. Reads are strict —
/// the value is untrusted input, and every mismatch is an error (a `SerialValueError`
/// inside the returned `DeserializeError`), never a panic: a key that is not a declared
/// field, a repeated key, a missing key of a required field, a variant name that is not
/// declared, a unit variant carrying data or a data variant without any, and a value of
/// the wrong kind for a field. An `Option` field reads a missing key, and a `null`, as
/// `None`.
///
/// Every field type must implement `DeserializableValue<L>` for every `L: Lang` — or
/// for the language named by `lang`; a field type without the impl is a compile error
/// at the field.
#[proc_macro_derive(DeserializableValue, attributes(serial))]
pub fn derive_deserializable_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    value_derive::expand_deserializable_value(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives techy's crate-private `ToSerialValue` for a wire struct or enum. Internal
/// to techy: the generated code refers to `crate::serialize::…` and compiles only
/// inside the techy crate. Every field and variant carries `#[serial(name = "…")]`;
/// see `techy/src/serialize/wire/mod.rs` for the traits and the wire shape.
#[doc(hidden)]
#[proc_macro_derive(ToSerialValue, attributes(serial))]
pub fn derive_to_serial_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    serial_value::expand_to(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives techy's crate-private `FromSerialValue` for a wire struct or enum — the
/// read direction of [`ToSerialValue`](macro@ToSerialValue), same attributes, strict
/// reads. Internal to techy.
#[doc(hidden)]
#[proc_macro_derive(FromSerialValue, attributes(serial))]
pub fn derive_from_serial_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    serial_value::expand_from(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// The reason `DiagnosticInfo` / `ToDiagnosticValue` reject generic types.
pub(crate) const DIAGNOSTIC_NO_GENERICS_REASON: &str =
    "diagnostic payloads are concrete data structs (DESIGN_RATIONALE.md [§dd-dr:errors])";

/// Rejects generic types, giving `reason` (for diagnostic payloads: `DiagnosticInfo`
/// requires `Any`, hence `'static`, and a generic payload has no single wire identity;
/// wire structs are concrete for the reason `serial_value.rs` gives).
pub(crate) fn ensure_no_generics(
    generics: &syn::Generics,
    derive_name: &str,
    reason: &str,
) -> syn::Result<()> {
    if !generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &generics.params,
            format!("#[derive({derive_name})] does not support generic types: {reason}"),
        ));
    }
    if let Some(where_clause) = &generics.where_clause {
        return Err(syn::Error::new_spanned(
            where_clause,
            format!("#[derive({derive_name})] does not support `where` clauses"),
        ));
    }
    Ok(())
}
