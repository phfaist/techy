//! Expansion of `#[derive(ToDiagnosticValue)]` (see the macro doc in lib.rs).

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    crate::ensure_no_generics(&input.generics, "ToDiagnosticValue", crate::DIAGNOSTIC_NO_GENERICS_REASON)?;

    let name = &input.ident;
    let data = match &input.data {
        Data::Enum(data) => data,
        Data::Struct(_) | Data::Union(_) => {
            return Err(syn::Error::new(
                input.ident.span(),
                "#[derive(ToDiagnosticValue)] supports field-less enums only; condition \
                 structs get their serialization from #[derive(DiagnosticInfo)], and \
                 other types implement `ToDiagnosticValue` by hand",
            ));
        }
    };
    if data.variants.is_empty() {
        return Err(syn::Error::new(
            input.ident.span(),
            "#[derive(ToDiagnosticValue)] requires at least one variant",
        ));
    }

    let arms = data
        .variants
        .iter()
        .map(|variant| {
            if !matches!(variant.fields, Fields::Unit) {
                return Err(syn::Error::new_spanned(
                    variant,
                    "#[derive(ToDiagnosticValue)] supports field-less variants only; a \
                     variant carrying data needs a hand-written `ToDiagnosticValue` impl",
                ));
            }
            let ident = &variant.ident;
            let wire = kebab_case(&ident.to_string());
            Ok(quote! { Self::#ident => #wire, })
        })
        .collect::<syn::Result<Vec<_>>>()?;

    Ok(quote! {
        #[automatically_derived]
        impl ::techy::__private::ToDiagnosticValue for #name {
            fn to_diagnostic_value(&self) -> ::techy::__private::DiagnosticValue {
                ::techy::__private::DiagnosticValue::Str(::techy::__private::String::from(
                    match self {
                        #(#arms)*
                    },
                ))
            }
        }
    })
}

/// PascalCase → kebab-case, acronym-aware: a boundary falls before an uppercase letter
/// that follows a lowercase letter or digit, or that starts the last capital of an
/// uppercase run followed by a lowercase letter (`EndOfInput` → `end-of-input`,
/// `EOFMarker` → `eof-marker`).
fn kebab_case(variant: &str) -> String {
    let chars: Vec<char> = variant.chars().collect();
    let mut out = String::with_capacity(variant.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let boundary = match i.checked_sub(1).map(|j| chars[j]) {
                None => false,
                Some(prev) => {
                    prev.is_lowercase()
                        || prev.is_ascii_digit()
                        || (prev.is_uppercase()
                            && chars.get(i + 1).is_some_and(|next| next.is_lowercase()))
                }
            };
            if boundary {
                out.push('-');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::kebab_case;

    #[test]
    fn kebab_case_variants() {
        assert_eq!(kebab_case("EndOfInput"), "end-of-input");
        assert_eq!(kebab_case("StrayGroupClose"), "stray-group-close");
        assert_eq!(kebab_case("EOFMarker"), "eof-marker");
        assert_eq!(kebab_case("EOF"), "eof");
        assert_eq!(kebab_case("Specials"), "specials");
        assert_eq!(kebab_case("Utf8Error"), "utf8-error");
    }
}
