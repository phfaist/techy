//! Expansion of the internal `#[derive(ToSerialValue)]` / `#[derive(FromSerialValue)]`
//! pair (see the macro docs in lib.rs): conversions between techy's crate-private wire
//! structs and `SerialValue`. Generated code refers to `crate::serialize::…` — the
//! derives are expanded inside techy only. The type model and the attribute grammar
//! ([`Model`], [`parse_model`]) are shared with the public value derives
//! (`value_derive.rs`), which generate different code from the same model.
//!
//! Wire shape (identical to what techy's serde bridge produces for the same shapes):
//! a struct → a map from each field's wire name to the field's value, in declaration
//! order, a field whose value is absent (an `Option::None`) omitted; a unit enum
//! variant → the string of its wire name; a newtype variant → a one-entry map from the
//! wire name to the payload; a struct variant → a one-entry map from the wire name to
//! the field map. Every field and every variant needs a mandatory
//! `#[serial(name = "…")]`; reads are strict (unknown, missing, or repeated keys and
//! unknown variants are errors).

use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Fields, Ident, LitStr, Type};

/// The model of the derived type, shared by both directions and by the public value
/// derives.
pub(crate) enum Model {
    Struct(Vec<NamedField>),
    Enum(Vec<Variant>),
}

pub(crate) struct NamedField {
    pub(crate) ident: Ident,
    pub(crate) name: LitStr,
    pub(crate) ty: Type,
}

pub(crate) struct Variant {
    pub(crate) ident: Ident,
    pub(crate) name: LitStr,
    pub(crate) kind: VariantKind,
}

pub(crate) enum VariantKind {
    Unit,
    Newtype(Type),
    Struct(Vec<NamedField>),
}

const NO_GENERICS_REASON: &str = "store a language-dependent part as an already encoded \
                                  ‘SerialValue’ field";

/// What a `serial` attribute on the derived type itself may contain.
pub(crate) enum TypeAttributePolicy {
    /// Nothing: every type-level `serial` attribute is an error — the internal wire
    /// derives, where `#[serial(name = "…")]` goes on fields and variants only.
    Rejected,
    /// `#[serial(lang = …)]`, naming the one language the generated impl is for — the
    /// public value derives.
    Lang,
}

/// The parsed derived type: its wire model and the type-level attribute, if any.
pub(crate) struct ParsedType {
    pub(crate) model: Model,
    /// The language named by `#[serial(lang = …)]`: only under
    /// [`TypeAttributePolicy::Lang`], and only when the attribute is given.
    pub(crate) lang: Option<Type>,
}

/// Parses the derived type into its wire model, checking the attribute grammar and
/// the supported shapes; `derive_name` names the derive in the error messages,
/// `generics_reason` is the fix that the rejection of a generic type suggests, and
/// `type_attributes` says what the type itself may contain.
pub(crate) fn parse_model(
    input: &DeriveInput,
    derive_name: &str,
    generics_reason: &str,
    type_attributes: TypeAttributePolicy,
) -> syn::Result<ParsedType> {
    crate::ensure_no_generics(&input.generics, derive_name, generics_reason)?;
    let lang = match type_attributes {
        TypeAttributePolicy::Rejected => {
            if let Some(attr) = input.attrs.iter().find(|attr| attr.path().is_ident("serial")) {
                return Err(syn::Error::new_spanned(
                    attr,
                    format!("‘#[derive({derive_name})]’ takes no ‘serial’ attribute on the type; \
                             put ‘#[serial(name = \"…\")]’ on every field and variant"),
                ));
            }
            None
        }
        TypeAttributePolicy::Lang => type_lang(&input.attrs)?,
    };
    let model = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => {
                let owner = format!("‘{}’", input.ident);
                Model::Struct(parse_named_fields(named, &owner, derive_name)?)
            }
            Fields::Unnamed(_) | Fields::Unit => {
                return Err(syn::Error::new(
                    input.ident.span(),
                    format!("‘#[derive({derive_name})]’ supports structs with named fields only"),
                ))
            }
        },
        Data::Enum(data) => {
            if data.variants.is_empty() {
                return Err(syn::Error::new(
                    input.ident.span(),
                    format!("‘#[derive({derive_name})]’ requires at least one variant"),
                ));
            }
            let mut variants = Vec::with_capacity(data.variants.len());
            for variant in &data.variants {
                let what = format!("variant ‘{}’", variant.ident);
                let name = wire_name(&variant.attrs, variant.ident.span(), &what, derive_name)?;
                let kind = match &variant.fields {
                    Fields::Unit => VariantKind::Unit,
                    Fields::Unnamed(unnamed) => {
                        if unnamed.unnamed.len() != 1 {
                            return Err(syn::Error::new_spanned(
                                unnamed,
                                format!(
                                    "‘#[derive({derive_name})]’ does not support the \
                                     tuple variant ‘{}’ with {} fields; use a unit \
                                     variant, a newtype variant or named fields",
                                    variant.ident,
                                    unnamed.unnamed.len()
                                ),
                            ));
                        }
                        let field = &unnamed.unnamed[0];
                        if field.attrs.iter().any(|attr| attr.path().is_ident("serial")) {
                            return Err(syn::Error::new_spanned(
                                field,
                                format!(
                                    "The payload of the newtype variant ‘{}’ may not have \
                                     a ‘serial’ attribute",
                                    variant.ident
                                ),
                            ));
                        }
                        VariantKind::Newtype(field.ty.clone())
                    }
                    Fields::Named(named) => {
                        VariantKind::Struct(parse_named_fields(named, &what, derive_name)?)
                    }
                };
                variants.push(Variant { ident: variant.ident.clone(), name, kind });
            }
            let owner = format!("variants of ‘{}’", input.ident);
            check_distinct(variants.iter().map(|v| &v.name), &owner)?;
            Model::Enum(variants)
        }
        Data::Union(data) => {
            return Err(syn::Error::new(
                data.union_token.span,
                format!("‘#[derive({derive_name})]’ supports structs and enums only"),
            ))
        }
    };
    Ok(ParsedType { model, lang })
}

// The optional `#[serial(lang = …)]` on the type: the one language the generated impl is
// for. Any other type-level `serial` key is an error.
fn type_lang(attrs: &[syn::Attribute]) -> syn::Result<Option<Type>> {
    let mut lang: Option<Type> = None;
    for attr in attrs {
        if !attr.path().is_ident("serial") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("lang") {
                if lang.is_some() {
                    return Err(meta.error("Duplicate key ‘lang’ in ‘#[serial(…)]’"));
                }
                lang = Some(meta.value()?.parse()?);
                Ok(())
            } else if meta.path.is_ident("name") {
                Err(meta.error(
                    "Key ‘name’ may not be used in ‘#[serial(…)]’ on the type (expected ‘lang’); \
                     put wire names on fields and variants",
                ))
            } else {
                Err(meta.error(format!(
                    "Unknown key ‘{}’ in ‘#[serial(…)]’ on the type (expected ‘lang’)",
                    crate::path_text(&meta.path)
                )))
            }
        })?;
    }
    Ok(lang)
}

// `owner` describes the struct or the struct variant that has the fields, in the error
// messages (as in `‘Size’` or `variant ‘Fixed’`).
fn parse_named_fields(
    named: &syn::FieldsNamed,
    owner: &str,
    derive_name: &str,
) -> syn::Result<Vec<NamedField>> {
    let mut fields = Vec::with_capacity(named.named.len());
    for field in &named.named {
        let ident = field.ident.clone().expect("named field has an ident");
        let what = format!("field ‘{ident}’");
        let name = wire_name(&field.attrs, ident.span(), &what, derive_name)?;
        fields.push(NamedField { ident, name, ty: field.ty.clone() });
    }
    check_distinct(fields.iter().map(|f| &f.name), &format!("fields of {owner}"))?;
    Ok(fields)
}

// The mandatory `#[serial(name = "…")]` of a field or variant; `what` describes the
// field or variant in the error messages (as in `field ‘width’`).
fn wire_name(
    attrs: &[syn::Attribute],
    span: proc_macro2::Span,
    what: &str,
    derive_name: &str,
) -> syn::Result<LitStr> {
    let mut name: Option<LitStr> = None;
    for attr in attrs {
        if !attr.path().is_ident("serial") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                if name.is_some() {
                    return Err(meta.error(format!(
                        "Duplicate key ‘name’ in ‘#[serial(…)]’ of {what}"
                    )));
                }
                let lit: LitStr = meta.value()?.parse()?;
                if lit.value().is_empty() {
                    return Err(syn::Error::new(lit.span(), format!("Empty wire name for {what}")));
                }
                name = Some(lit);
                Ok(())
            } else {
                Err(meta.error(format!(
                    "Unknown key ‘{}’ in ‘#[serial(…)]’ of {what} (expected ‘name’)",
                    crate::path_text(&meta.path)
                )))
            }
        })?;
    }
    name.ok_or_else(|| {
        syn::Error::new(
            span,
            format!(
                "Missing ‘#[serial(name = \"…\")]’ on {what} (required by \
                 ‘#[derive({derive_name})]’)"
            ),
        )
    })
}

// `what` describes the set of names in the error message (as in `fields of ‘Size’`).
fn check_distinct<'a>(names: impl Iterator<Item = &'a LitStr>, what: &str) -> syn::Result<()> {
    let mut seen: Vec<String> = Vec::new();
    for name in names {
        let value = name.value();
        if seen.contains(&value) {
            return Err(syn::Error::new(
                name.span(),
                format!("Duplicate wire name ‘{value}’ among the {what}"),
            ));
        }
        seen.push(value);
    }
    Ok(())
}

// --- ToSerialValue --------------------------------------------------------------------------

pub(crate) fn expand_to(input: DeriveInput) -> syn::Result<TokenStream> {
    let ParsedType { model, .. } =
        parse_model(&input, "ToSerialValue", NO_GENERICS_REASON, TypeAttributePolicy::Rejected)?;
    let name = &input.ident;
    let body = match &model {
        Model::Struct(fields) => {
            let write = write_fields(fields, |field| quote! { &self.#field });
            quote! { #write }
        }
        Model::Enum(variants) => {
            let arms = variants.iter().map(|variant| {
                let ident = &variant.ident;
                let wire = &variant.name;
                match &variant.kind {
                    VariantKind::Unit => quote! {
                        Self::#ident => ::core::result::Result::Ok(
                            crate::serialize::wire::unit_variant(#wire),
                        ),
                    },
                    VariantKind::Newtype(ty) => {
                        let convert = quote_spanned! {ty.span()=>
                            crate::serialize::wire::ToSerialValue::to_serial_value(__payload)?
                        };
                        quote! {
                            Self::#ident(__payload) => ::core::result::Result::Ok(
                                crate::serialize::wire::data_variant(#wire, #convert),
                            ),
                        }
                    }
                    VariantKind::Struct(fields) => {
                        let idents = fields.iter().map(|f| &f.ident);
                        let write = write_fields(fields, |field| quote! { #field });
                        quote! {
                            Self::#ident { #(#idents),* } => {
                                let __fields = { #write }?;
                                ::core::result::Result::Ok(
                                    crate::serialize::wire::data_variant(#wire, __fields),
                                )
                            }
                        }
                    }
                }
            });
            quote! { match self { #(#arms)* } }
        }
    };
    Ok(quote! {
        #[automatically_derived]
        impl crate::serialize::wire::ToSerialValue for #name {
            fn to_serial_value(
                &self,
            ) -> ::core::result::Result<crate::serialize::SerialValue, crate::serialize::SerialValueError> {
                #body
            }
        }
    })
}

// Writes named fields into a `FieldWriter` and finishes it; `access` produces the
// expression yielding `&FieldType` for a field ident.
fn write_fields(fields: &[NamedField], access: impl Fn(&Ident) -> TokenStream) -> TokenStream {
    let len = fields.len();
    let writes = fields.iter().map(|field| {
        let wire = &field.name;
        let value = access(&field.ident);
        quote_spanned! {field.ty.span()=>
            __writer.field(#wire, #value)?;
        }
    });
    quote! {
        let mut __writer = crate::serialize::wire::FieldWriter::with_capacity(#len);
        #(#writes)*
        ::core::result::Result::Ok(__writer.finish())
    }
}

// --- FromSerialValue ------------------------------------------------------------------------

pub(crate) fn expand_from(input: DeriveInput) -> syn::Result<TokenStream> {
    let ParsedType { model, .. } =
        parse_model(&input, "FromSerialValue", NO_GENERICS_REASON, TypeAttributePolicy::Rejected)?;
    let name = &input.ident;
    let type_name = name.to_string();
    let body = match &model {
        Model::Struct(fields) => {
            let read = read_fields(fields, &type_name, quote! { #name });
            quote! { #read }
        }
        Model::Enum(variants) => {
            let names = variants.iter().map(|v| &v.name);
            let arms = variants.iter().map(|variant| {
                let ident = &variant.ident;
                let wire = &variant.name;
                match &variant.kind {
                    VariantKind::Unit => quote! {
                        #wire => {
                            crate::serialize::wire::expect_unit_variant(#wire, __payload)?;
                            ::core::result::Result::Ok(Self::#ident)
                        }
                    },
                    VariantKind::Newtype(ty) => {
                        let convert = quote_spanned! {ty.span()=>
                            crate::serialize::wire::FromSerialValue::from_serial_value(__data)?
                        };
                        quote! {
                            #wire => {
                                let __data = crate::serialize::wire::expect_data_variant(#wire, __payload)?;
                                ::core::result::Result::Ok(Self::#ident(#convert))
                            }
                        }
                    }
                    VariantKind::Struct(fields) => {
                        let what = format!("{type_name}::{ident}");
                        let read = read_fields(fields, &what, quote! { Self::#ident });
                        quote! {
                            #wire => {
                                let __value = crate::serialize::wire::expect_data_variant(#wire, __payload)?;
                                #read
                            }
                        }
                    }
                }
            });
            quote! {
                const __VARIANTS: &[&str] = &[#(#names),*];
                let (__name, __payload) =
                    crate::serialize::wire::read_variant(__value, #type_name, __VARIANTS)?;
                match __name {
                    #(#arms)*
                    __other => ::core::result::Result::Err(
                        crate::serialize::wire::unknown_variant(__other, __VARIANTS),
                    ),
                }
            }
        }
    };
    Ok(quote! {
        #[automatically_derived]
        impl crate::serialize::wire::FromSerialValue for #name {
            fn from_serial_value(
                __value: &crate::serialize::SerialValue,
            ) -> ::core::result::Result<Self, crate::serialize::SerialValueError> {
                #body
            }
        }
    })
}

// Reads named fields from `__value` through a `FieldReader` and builds
// `constructor { … }`; `what` names the type (or variant) in the shape error.
fn read_fields(fields: &[NamedField], what: &str, constructor: TokenStream) -> TokenStream {
    let names = fields.iter().map(|f| &f.name);
    let reads = fields.iter().map(|field| {
        let ident = &field.ident;
        let wire = &field.name;
        let local = format_ident!("__field_{}", ident);
        let read = quote_spanned! {field.ty.span()=>
            let #local = __reader.field(#wire)?;
        };
        (read, quote! { #ident: #local, })
    });
    let (reads, inits): (Vec<_>, Vec<_>) = reads.unzip();
    quote! {
        const __FIELDS: &[&str] = &[#(#names),*];
        let __reader = crate::serialize::wire::FieldReader::new(__value, #what, __FIELDS)?;
        #(#reads)*
        ::core::result::Result::Ok(#constructor { #(#inits)* })
    }
}

#[cfg(test)]
mod tests {
    use syn::{parse_quote, DeriveInput};

    use super::{expand_from, expand_to};

    // The internal derives take no attribute on the type at all — the public derives'
    // `lang` included.
    #[test]
    fn the_internal_derives_refuse_every_type_level_attribute() {
        let inputs: [DeriveInput; 2] = [
            parse_quote! {
                #[serial(lang = L)]
                struct S {
                    #[serial(name = "a")]
                    a: u32,
                }
            },
            parse_quote! {
                #[serial(name = "s")]
                struct S {
                    #[serial(name = "a")]
                    a: u32,
                }
            },
        ];
        for input in inputs {
            expand_to(input.clone()).expect_err("ToSerialValue accepts");
            expand_from(input).expect_err("FromSerialValue accepts");
        }
        // The same type without the type-level attribute is accepted, so the attribute
        // alone causes the rejection.
        let input: DeriveInput = parse_quote! {
            struct S {
                #[serial(name = "a")]
                a: u32,
            }
        };
        expand_to(input.clone()).expect("ToSerialValue rejects");
        expand_from(input).expect("FromSerialValue rejects");
    }
}
