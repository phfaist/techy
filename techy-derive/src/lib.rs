//! Derive macros for `techy`: declaring structured diagnostic conditions, and
//! serializing a language's own value types.
//!
//! This is the build-time companion crate of `techy`, which re-exports every public
//! derive next to the trait it implements. Depend on `techy` and use those re-exports
//! rather than depending on this crate directly.
//!
//! Re-exported from `techy::error`:
//!
//! - [`DiagnosticInfo`](macro@DiagnosticInfo) — placed on a diagnostic condition
//!   struct. It generates the `DiagnosticInfo` implementation (the condition's wire
//!   identifier and its serializable payload), and optionally a `Display`
//!   implementation and a `new()` constructor.
//! - [`ToDiagnosticValue`](macro@ToDiagnosticValue) — placed on a field-less enum used
//!   as a condition field. Each variant serializes as its kebab-cased name.
//!
//! Re-exported from `techy::serialize`:
//!
//! - [`SerializableValue`](macro@SerializableValue) and
//!   [`DeserializableValue`](macro@DeserializableValue) — placed on a plain-data struct
//!   or enum. They generate the write and the read half of the conversion between the
//!   type and techy's format-independent value model, converting each field through
//!   that field type's own implementation.
//!
//! The crate also provides a hidden pair of derives (`ToSerialValue` /
//! `FromSerialValue`) whose generated code compiles only inside `techy` itself.

mod diagnostic_info;
mod serial_value;
mod to_value;
mod value_derive;

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

/// Derives `techy::error::DiagnosticInfo` for a diagnostic condition struct.
///
/// A condition is one plain data struct holding the facts about a single problem the
/// parser can report. This derive turns such a struct into a reportable condition: it
/// supplies the condition's wire identifier, projects the fields into the serializable
/// payload, and optionally writes the `Display` message and a constructor.
///
/// # Examples
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
/// `UnresolvableCommand::new("world", '\\')` then builds the condition, it displays as
/// `cannot resolve command ‘\world’`, and its serializable payload is the two-entry map
/// `{"name": "world", "escape_char": "\\"}`.
///
/// # Attributes
///
/// On the struct, `#[diagnostic(…)]` accepts these keys, in any order, each at most
/// once:
///
/// - `id = "<dotted name>"` — **required**. The value of the `IDENTIFIER` constant: the
///   condition's stable identity in serialized output and in configuration that selects
///   conditions by name. It must be a non-empty dotted name without whitespace and with
///   no leading or trailing dot, conventionally
///   `<crate-or-language>.<area>.<condition>`. It is never derived from the type name,
///   so that renaming the struct is an internal refactoring rather than a silent change
///   of the identity.
/// - `message = "<format string>"` — optional; without it, no `Display` implementation
///   is generated and you write one by hand. `{field}` interpolates a field by name,
///   optionally with a format specification (`{width:04}`); `{{` and `}}` are literal
///   braces. Positional placeholders (`{}`, `{0}`) are rejected: the message refers to
///   fields by name only. Write `Display` by hand when the wording needs a `match`, a
///   conditional, or a conversion.
/// - `no_constructor` — optional flag; suppresses the generated `new()`.
///
/// On a field, `#[diagnostic(key = "<name>")]` sets that field's serialization key.
/// The default is the field's own name. Use it when the key must differ from the Rust
/// name (a field `ch` serialized as `"char"`, say): the key is part of the stable
/// serialized form, whereas the field name is free to change. The key must be
/// non-empty and contain no whitespace.
///
/// # What is generated
///
/// - `impl DiagnosticInfo`: `IDENTIFIER` is the `id` string, and `serializable_data()`
///   returns a `DiagnosticValue::Map` with one entry per field, in declaration order,
///   keyed by the field's serialization key, whose value comes from
///   `techy::error::ToDiagnosticValue`. A struct with no fields returns an empty map.
/// - `impl Display`, only when `message` is given.
/// - `pub fn new(…) -> Self`, taking one `impl Into<FieldType>` parameter per field in
///   declaration order, unless `no_constructor` is given. Conditions are usually marked
///   `#[non_exhaustive]`, which stops other crates from using a struct literal, so this
///   constructor is how they build the condition.
///
/// # Requirements
///
/// - The type is a struct with named fields, or a unit struct.
/// - It has no generic parameters and no `where` clause.
/// - No field uses a raw identifier (`r#type`): a field's name is both its default
///   serialization key and the name the message interpolates.
/// - Every field type implements `techy::error::ToDiagnosticValue`. The trait is
///   implemented for booleans, integers, `char`, strings, `Option`, slices and `Vec`,
///   references, and the value type itself; other field types need their own
///   implementation, or the [`ToDiagnosticValue`](macro@ToDiagnosticValue) derive.
/// - The trait also requires `Clone`, `Debug`, `Send`, `Sync`, `'static`, and
///   `Display`, so derive or implement those as well (`Display` may come from
///   `message`).
///
/// # Compile errors
///
/// Every rejection is reported at the offending item:
///
/// - `does not support generic types`, `does not support "where" clauses` — the type is
///   not concrete.
/// - `requires named fields` — the type is a tuple struct.
/// - `supports structs only` — the type is an enum or a union. A field-less enum used as
///   a condition *field* wants the [`ToDiagnosticValue`](macro@ToDiagnosticValue) derive
///   instead.
/// - `missing "#[diagnostic(id = …)]"`, `the identifier must be a namespaced dotted
///   name` — the `id` key is absent or malformed.
/// - `message references unknown field`, and the unclosed- or unmatched-brace errors —
///   the `message` format string does not match the struct.
/// - An unsatisfied `ToDiagnosticValue` bound, reported at a field's type — that field
///   cannot be serialized. Serializability of the payload is checked by the compiler,
///   not by the macro.
#[proc_macro_derive(DiagnosticInfo, attributes(diagnostic))]
pub fn derive_diagnostic_info(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    diagnostic_info::expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives `techy::error::ToDiagnosticValue` for a field-less enum.
///
/// Each variant serializes as a string holding the kebab-cased variant name, so a field
/// of this type can appear in a condition struct that derives
/// [`DiagnosticInfo`](macro@DiagnosticInfo). The conversion is acronym-aware:
/// `EndOfInput` becomes `"end-of-input"` and `EOFMarker` becomes `"eof-marker"`.
///
/// # Examples
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
/// # Requirements
///
/// The type is an enum with at least one variant, every variant is field-less, and the
/// enum has no generic parameters and no `where` clause. The derive takes no
/// attributes: the wire name of a variant is always its kebab-cased Rust name.
///
/// Anything else implements the trait by hand — a variant that has data, or a type that
/// is not an enum. A condition struct does not need this derive at all; it gets its
/// serialization from [`DiagnosticInfo`](macro@DiagnosticInfo).
#[proc_macro_derive(ToDiagnosticValue)]
pub fn derive_to_diagnostic_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    to_value::expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives `techy::serialize::SerializableValue` for a plain-data struct or enum.
///
/// This is the write half of the conversion between the type and techy's
/// format-independent value model; the read half is the companion derive
/// [`DeserializableValue`](macro@DeserializableValue), and the two are normally derived
/// together. The generated `serialize_value` converts every field through that field
/// type's own `SerializableValue` implementation, passing the serialization context
/// along, and assembles the result described under *The serialized form* below.
///
/// # Examples
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
/// A `Placement` whose `kind` is `Kind::Sized { width: 3 }`, whose `width` is `10`, and
/// whose `note` is `None` serializes to the map
/// `{"kind": {"sized": {"width": 3}}, "width": 10}`.
///
/// # Attributes
///
/// `#[serial(name = "<wire name>")]` on **every** field and **every** variant is
/// required; a field or a variant without one is a compile error. A wire name is what a
/// program writes and later reads back, possibly across its own versions, so it is
/// chosen deliberately and never taken from the Rust identifier, which is free to be
/// renamed. Wire names must be non-empty and distinct within the type. The payload of a
/// newtype variant takes no name of its own: the variant's name is its key.
///
/// `#[serial(lang = <type>)]` on the type itself is optional, and is the only
/// type-level `serial` key. It restricts the generated implementation to one language,
/// as described under *The language* below. The named type must implement
/// `techy::serialize::SerializableLang`, which the generated signatures require;
/// otherwise the compiler reports an unsatisfied bound.
///
/// # The serialized form
///
/// - A struct becomes a map from each field's wire name to that field's value, in
///   declaration order. A field whose value is absent — an `Option` that is `None` — is
///   left out of the map entirely, rather than written as a key with a null value.
/// - A unit variant becomes the string of its wire name: `"plain"`.
/// - A newtype variant becomes a one-entry map from its wire name to the payload's
///   value: `{"named": "x"}`.
/// - A variant with named fields becomes a one-entry map from its wire name to the map
///   of its fields: `{"sized": {"width": 3}}`.
///
/// This is the form techy's own structures use. It is also the form the `serde` bridge
/// produces for the corresponding serde shapes, that is, with `rename`,
/// `skip_serializing_if = "Option::is_none"`, `default`, and `deny_unknown_fields`.
///
/// # The language
///
/// Without a type-level attribute the generated implementation covers every language:
/// `impl<L: Lang> SerializableValue<L> for T`. Every field type must then implement
/// `SerializableValue<L>` for every `L: Lang`, as the implementations for `bool`, the
/// integers, `char`, `String`, `Option<T>`, `Vec<T>`, and `SerialValue` do, and as every
/// type derived this way does.
///
/// A field whose conversion exists for one language only needs that language named on
/// the type. `#[serial(lang = MyLang)]` generates
/// `impl SerializableValue<MyLang> for T` instead, and the type is then usable by that
/// language alone. The common case is a span: `SourceSpan<O>` converts for the languages
/// whose `SourceOrigin` is `O`, and interns its source through the serialization context
/// just as a hand-written implementation would.
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
/// # Requirements
///
/// - The type is a struct with named fields, or an enum with at least one variant, each
///   variant being a unit variant, a newtype variant (exactly one unnamed field), or a
///   variant with named fields.
/// - It has no generic parameters and no `where` clause. A value whose type depends on
///   the language therefore cannot be a field of a type derived for every language;
///   store it as an already converted `SerialValue`, or name the language with
///   `#[serial(lang = …)]`.
/// - Every field type implements `SerializableValue` for the language or languages the
///   generated implementation covers.
///
/// # Compile errors
///
/// Every rejection is reported at the offending item: a missing, empty, or repeated
/// wire name; a wire name on a newtype variant's payload; a `serial` key other than
/// `name` on a field or variant, or other than `lang` on the type; a tuple struct, a
/// unit struct, a tuple variant with several fields, a union, or an empty enum; and a
/// generic type or a `where` clause. An unsatisfied `SerializableValue` bound reported
/// at a field's type means that field type has no conversion for the language in
/// question.
///
/// # When to write the implementation by hand
///
/// When the serialized form must differ from the type's own layout: a value computed
/// from the type, a field storing a shared handle, or a form that must stay unchanged
/// while the type changes. Write `serialize_value` by hand in that case; where it helps,
/// convert through a separate struct that has the serialized layout and derives this
/// trait.
#[proc_macro_derive(SerializableValue, attributes(serial))]
pub fn derive_serializable_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    value_derive::expand_serializable_value(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives `techy::serialize::DeserializableValue` for a plain-data struct or enum.
///
/// This is the read half of the conversion, and reads exactly the form that
/// [`SerializableValue`](macro@SerializableValue) writes. It accepts the same
/// attributes and the same type shapes, documented on that derive; the two are normally
/// derived together.
///
/// The generated implementation is `impl<L: Lang> DeserializableValue<L> for T`, or
/// `impl DeserializableValue<MyLang> for T` when the type has
/// `#[serial(lang = MyLang)]`. Its `deserialize_value` reads every field through that
/// field type's own `DeserializableValue` implementation, passing the deserialization
/// context along. Every field type must therefore implement `DeserializableValue` for
/// the language or languages the implementation covers; a field type without it is a
/// compile error at the field.
///
/// # Errors
///
/// The value being read is untrusted input, so reads are strict: every mismatch is
/// returned as an error, never a panic. The returned `DeserializeError` holds a
/// `SerialValueError` describing the mismatch, which is one of: a key that is not a
/// declared field, a repeated key, a missing key for a required field, an undeclared
/// variant name, a unit variant that has data or a data variant that has none, or a
/// value of the wrong kind for a field.
///
/// An `Option` field is the exception to a missing key being an error: both a missing
/// key and a null value read as `None`.
#[proc_macro_derive(DeserializableValue, attributes(serial))]
pub fn derive_deserializable_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    value_derive::expand_deserializable_value(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives techy's crate-private `ToSerialValue` for a wire struct or enum.
///
/// Internal to techy: the generated code names `crate::serialize::…` items and compiles
/// only inside the techy crate. Every field and variant needs `#[serial(name = "…")]`;
/// see `techy/src/serialize/wire/mod.rs` for the traits and the serialized form.
#[doc(hidden)]
#[proc_macro_derive(ToSerialValue, attributes(serial))]
pub fn derive_to_serial_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    serial_value::expand_to(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives techy's crate-private `FromSerialValue` for a wire struct or enum.
///
/// The read direction of [`ToSerialValue`](macro@ToSerialValue): the same attributes,
/// and strict reads. Internal to techy.
#[doc(hidden)]
#[proc_macro_derive(FromSerialValue, attributes(serial))]
pub fn derive_from_serial_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    serial_value::expand_from(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

// Why the two diagnostic derives reject generic types: `DiagnosticInfo` requires `Any`,
// hence `'static`, and a generic payload has no single wire identity.
pub(crate) const DIAGNOSTIC_NO_GENERICS_REASON: &str =
    "diagnostic payloads are concrete data structs (DESIGN_RATIONALE.md [§dd-dr:errors])";

// Rejects a generic type or a `where` clause on the derived type; `reason` completes
// the error message (each derive supplies its own).
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
