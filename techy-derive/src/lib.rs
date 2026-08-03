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

mod diagnostic_info;
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
///   keyed by field name. A field whose type does not implement `ToDiagnosticValue`
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

/// Rejects generic types: diagnostic payloads are concrete data structs
/// (`DiagnosticInfo` requires `Any`, hence `'static`; and a generic payload has no
/// single wire identity).
pub(crate) fn ensure_no_generics(
    generics: &syn::Generics,
    derive_name: &str,
) -> syn::Result<()> {
    if !generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &generics.params,
            format!(
                "#[derive({derive_name})] does not support generic types: \
                 diagnostic payloads are concrete data structs (DESIGN_RATIONALE.md [§dd-dr:errors])"
            ),
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
