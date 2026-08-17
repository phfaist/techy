//! Derive macros for techy's structured diagnostic conditions.
//!
//! This is the build-time companion crate of `techy`. `techy` re-exports both derives
//! from `techy::error`, next to the traits they implement — depend on `techy` and use
//! the re-exports rather than depending on this crate directly.
//!
//! - [`DiagnosticInfo`](macro@DiagnosticInfo) — on a condition data struct: generates
//!   the `DiagnosticInfo` impl (`IDENTIFIER`, `serializable_data()`), optionally a
//!   `Display` impl from a message format string, and the `new()` constructor.
//! - [`ToDiagnosticValue`](macro@ToDiagnosticValue) — on a field-less payload enum:
//!   serializes as the kebab-cased variant name.
//!
//! The crate also carries techy-internal derives (`ToSerialValue` / `FromSerialValue`,
//! hidden) whose generated code compiles only inside techy itself.

mod diagnostic_info;
mod serial_value;
mod to_value;

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
