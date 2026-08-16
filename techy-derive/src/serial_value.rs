//! Expansion of the internal `#[derive(ToSerialValue)]` / `#[derive(FromSerialValue)]`
//! pair (see the macro docs in lib.rs): conversions between techy's crate-private wire
//! structs and `SerialValue`. Generated code refers to `crate::serialize::…` — the
//! derives are expanded inside techy only.
//!
//! Wire shape (identical to what techy's serde bridge produces for the same shapes):
//! a struct → a map from each field's wire name to the field's value, in declaration
//! order, a field whose value is absent (an `Option::None`) omitted; a unit enum
//! variant → the string of its wire name; a newtype variant → a one-entry map from the
//! wire name to the payload; a struct variant → a one-entry map from the wire name to
//! the field map. Every field and every variant carries a mandatory
//! `#[serial(name = "…")]`; reads are strict (unknown, missing, or repeated keys and
//! unknown variants are errors).

use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Fields, Ident, LitStr, Type};

/// The wire model of the derived type, shared by both directions.
enum Model {
    Struct(Vec<NamedField>),
    Enum(Vec<Variant>),
}

struct NamedField {
    ident: Ident,
    name: LitStr,
    ty: Type,
}

struct Variant {
    ident: Ident,
    name: LitStr,
    kind: VariantKind,
}

enum VariantKind {
    Unit,
    Newtype(Type),
    Struct(Vec<NamedField>),
}

const NO_GENERICS_REASON: &str = "wire structs are concrete: a language-dependent part is \
                                  carried as an already-encoded `SerialValue` field";

fn parse_model(input: &DeriveInput, derive_name: &str) -> syn::Result<Model> {
    crate::ensure_no_generics(&input.generics, derive_name, NO_GENERICS_REASON)?;
    if let Some(attr) = input.attrs.iter().find(|attr| attr.path().is_ident("serial")) {
        return Err(syn::Error::new_spanned(
            attr,
            format!("#[derive({derive_name})] takes no `serial` attribute on the type itself; \
                     `#[serial(name = \"…\")]` goes on every field and variant"),
        ));
    }
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => {
                let fields = parse_named_fields(named, derive_name)?;
                Ok(Model::Struct(fields))
            }
            Fields::Unnamed(_) | Fields::Unit => Err(syn::Error::new(
                input.ident.span(),
                format!("#[derive({derive_name})] supports structs with named fields only \
                         (each field carries its wire name)"),
            )),
        },
        Data::Enum(data) => {
            if data.variants.is_empty() {
                return Err(syn::Error::new(
                    input.ident.span(),
                    format!("#[derive({derive_name})] requires at least one variant"),
                ));
            }
            let mut variants = Vec::with_capacity(data.variants.len());
            for variant in &data.variants {
                let name = wire_name(&variant.attrs, variant.ident.span(), "variant", derive_name)?;
                let kind = match &variant.fields {
                    Fields::Unit => VariantKind::Unit,
                    Fields::Unnamed(unnamed) => {
                        if unnamed.unnamed.len() != 1 {
                            return Err(syn::Error::new_spanned(
                                unnamed,
                                format!("#[derive({derive_name})] supports unit variants, newtype \
                                         variants (one unnamed field), and variants with named \
                                         fields; use named fields for several values"),
                            ));
                        }
                        let field = &unnamed.unnamed[0];
                        if field.attrs.iter().any(|attr| attr.path().is_ident("serial")) {
                            return Err(syn::Error::new_spanned(
                                field,
                                "the payload of a newtype variant carries no wire name of its own \
                                 (the variant's name is its key)",
                            ));
                        }
                        VariantKind::Newtype(field.ty.clone())
                    }
                    Fields::Named(named) => VariantKind::Struct(parse_named_fields(named, derive_name)?),
                };
                variants.push(Variant { ident: variant.ident.clone(), name, kind });
            }
            check_distinct(variants.iter().map(|v| &v.name), "variant")?;
            Ok(Model::Enum(variants))
        }
        Data::Union(data) => Err(syn::Error::new(
            data.union_token.span,
            format!("#[derive({derive_name})] supports structs and enums only"),
        )),
    }
}

fn parse_named_fields(named: &syn::FieldsNamed, derive_name: &str) -> syn::Result<Vec<NamedField>> {
    let mut fields = Vec::with_capacity(named.named.len());
    for field in &named.named {
        let ident = field.ident.clone().expect("named field has an ident");
        let name = wire_name(&field.attrs, ident.span(), "field", derive_name)?;
        fields.push(NamedField { ident, name, ty: field.ty.clone() });
    }
    check_distinct(fields.iter().map(|f| &f.name), "field")?;
    Ok(fields)
}

/// The mandatory `#[serial(name = "…")]` of a field or variant.
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
                    return Err(meta.error("duplicate `name`"));
                }
                let lit: LitStr = meta.value()?.parse()?;
                if lit.value().is_empty() {
                    return Err(syn::Error::new(lit.span(), "the wire name must not be empty"));
                }
                name = Some(lit);
                Ok(())
            } else {
                Err(meta.error("unknown `serial` key; expected `name`"))
            }
        })?;
    }
    name.ok_or_else(|| {
        syn::Error::new(
            span,
            format!("#[derive({derive_name})]: every {what} needs `#[serial(name = \"…\")]` — \
                     wire names are chosen deliberately, never taken from Rust identifiers"),
        )
    })
}

fn check_distinct<'a>(names: impl Iterator<Item = &'a LitStr>, what: &str) -> syn::Result<()> {
    let mut seen: Vec<String> = Vec::new();
    for name in names {
        let value = name.value();
        if seen.contains(&value) {
            return Err(syn::Error::new(
                name.span(),
                format!("wire name `{value}` is used by another {what} of the same type"),
            ));
        }
        seen.push(value);
    }
    Ok(())
}

// --- ToSerialValue --------------------------------------------------------------------------

pub(crate) fn expand_to(input: DeriveInput) -> syn::Result<TokenStream> {
    let model = parse_model(&input, "ToSerialValue")?;
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

/// Writes named fields into a `FieldWriter` and finishes it; `access` produces the
/// expression yielding `&FieldType` for a field ident.
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
    let model = parse_model(&input, "FromSerialValue")?;
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

/// Reads named fields from `__value` through a `FieldReader` and builds `constructor
/// { … }`; `what` names the type (or variant) in the shape error.
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
