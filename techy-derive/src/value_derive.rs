//! Expansion of the public `#[derive(SerializableValue)]` /
//! `#[derive(DeserializableValue)]` pair (see the macro docs in lib.rs): the impls of
//! techy's value capability traits for a plain-data struct or enum, with the
//! serialization context passed to every field's own conversion. The type model and
//! the attribute grammar are those of the internal wire derives
//! ([`parse_model`](crate::serial_value::parse_model)); only the generated code
//! differs — every field converts through its own `SerializableValue<L>` /
//! `DeserializableValue<L>` impl with the context, and the generated impl is for every
//! `L: Lang`.
//!
//! Generated code names the capability traits and the language bound by their
//! canonical public paths (`::techy::serialize::…`, `::techy::core::Lang`) — a
//! `#[doc(hidden)]` import path to a public trait makes semver tooling report the
//! trait as sealed — and everything else (contexts, errors, the value type, the field
//! and variant helpers) through `::techy::__private::…`. The impl's type parameter is
//! `__L`, a name no user type shadows.

use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{DeriveInput, Ident};

use crate::serial_value::{parse_model, Model, NamedField, VariantKind};

// --- SerializableValue ------------------------------------------------------------------

pub(crate) fn expand_serializable_value(input: DeriveInput) -> syn::Result<TokenStream> {
    let model = parse_model(&input, "SerializableValue")?;
    let name = &input.ident;
    let body = match &model {
        Model::Struct(fields) => {
            let write = write_fields(fields, |field| quote! { &self.#field });
            quote! {
                #write
                ::core::result::Result::Ok(__writer.finish())
            }
        }
        Model::Enum(variants) => {
            let arms = variants.iter().map(|variant| {
                let ident = &variant.ident;
                let wire = &variant.name;
                match &variant.kind {
                    VariantKind::Unit => quote! {
                        Self::#ident => ::core::result::Result::Ok(
                            ::techy::__private::unit_variant(#wire),
                        ),
                    },
                    VariantKind::Newtype(ty) => {
                        let convert = quote_spanned! {ty.span()=>
                            <#ty as ::techy::serialize::SerializableValue<__L>>::serialize_value(
                                __payload,
                                __cx,
                            )?
                        };
                        quote! {
                            Self::#ident(__payload) => ::core::result::Result::Ok(
                                ::techy::__private::data_variant(#wire, #convert),
                            ),
                        }
                    }
                    VariantKind::Struct(fields) => {
                        let idents = fields.iter().map(|f| &f.ident);
                        let write = write_fields(fields, |field| quote! { #field });
                        quote! {
                            Self::#ident { #(#idents),* } => {
                                #write
                                ::core::result::Result::Ok(
                                    ::techy::__private::data_variant(#wire, __writer.finish()),
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
        impl<__L: ::techy::core::Lang> ::techy::serialize::SerializableValue<__L> for #name {
            fn serialize_value(
                &self,
                __cx: &mut ::techy::__private::SerializeContext<'_, __L>,
            ) -> ::core::result::Result<
                ::techy::__private::SerialValue,
                ::techy::__private::SerializeError,
            >
            where
                __L: ::techy::serialize::SerializableLang,
            {
                #body
            }
        }
    })
}

/// The statements declaring `__writer` and writing the named fields into it through
/// their `SerializableValue<__L>` impls with the context (the caller finishes the
/// writer — the `?`s propagate to the generated method directly, whose error type is
/// the helper's); `access` produces the expression yielding `&FieldType` for a field
/// ident.
fn write_fields(fields: &[NamedField], access: impl Fn(&Ident) -> TokenStream) -> TokenStream {
    let len = fields.len();
    let writes = fields.iter().map(|field| {
        let wire = &field.name;
        let value = access(&field.ident);
        quote_spanned! {field.ty.span()=>
            __writer.value_field(#wire, #value, __cx)?;
        }
    });
    quote! {
        let mut __writer = ::techy::__private::FieldWriter::with_capacity(#len);
        #(#writes)*
    }
}

// --- DeserializableValue ----------------------------------------------------------------

pub(crate) fn expand_deserializable_value(input: DeriveInput) -> syn::Result<TokenStream> {
    let model = parse_model(&input, "DeserializableValue")?;
    let name = &input.ident;
    let type_name = name.to_string();
    let body = match &model {
        Model::Struct(fields) => read_fields(fields, &type_name, quote! { #name }),
        Model::Enum(variants) => {
            let names = variants.iter().map(|v| &v.name);
            let arms = variants.iter().map(|variant| {
                let ident = &variant.ident;
                let wire = &variant.name;
                match &variant.kind {
                    VariantKind::Unit => quote! {
                        #wire => {
                            ::techy::__private::expect_unit_variant(#wire, __payload)?;
                            ::core::result::Result::Ok(Self::#ident)
                        }
                    },
                    VariantKind::Newtype(ty) => {
                        let convert = quote_spanned! {ty.span()=>
                            <#ty as ::techy::serialize::DeserializableValue<__L>>::deserialize_value(
                                __data,
                                __cx,
                            )?
                        };
                        quote! {
                            #wire => {
                                let __data = ::techy::__private::expect_data_variant(#wire, __payload)?;
                                ::core::result::Result::Ok(Self::#ident(#convert))
                            }
                        }
                    }
                    VariantKind::Struct(fields) => {
                        let what = format!("{type_name}::{ident}");
                        let read = read_fields(fields, &what, quote! { Self::#ident });
                        quote! {
                            #wire => {
                                let __value = ::techy::__private::expect_data_variant(#wire, __payload)?;
                                #read
                            }
                        }
                    }
                }
            });
            quote! {
                const __VARIANTS: &[&str] = &[#(#names),*];
                let (__name, __payload) =
                    ::techy::__private::read_variant(__value, #type_name, __VARIANTS)?;
                match __name {
                    #(#arms)*
                    __other => ::core::result::Result::Err(::core::convert::From::from(
                        ::techy::__private::unknown_variant(__other, __VARIANTS),
                    )),
                }
            }
        }
    };
    Ok(quote! {
        #[automatically_derived]
        impl<__L: ::techy::core::Lang> ::techy::serialize::DeserializableValue<__L> for #name {
            fn deserialize_value(
                __value: &::techy::__private::SerialValue,
                __cx: &mut ::techy::__private::DeserializeContext<'_, __L>,
            ) -> ::core::result::Result<Self, ::techy::__private::DeserializeError>
            where
                __L: ::techy::serialize::SerializableLang,
            {
                #body
            }
        }
    })
}

/// Reads named fields from `__value` through a `FieldReader` and their
/// `DeserializableValue<__L>` impls with the context, and builds `constructor { … }`;
/// `what` names the type (or variant) in the shape error.
fn read_fields(fields: &[NamedField], what: &str, constructor: TokenStream) -> TokenStream {
    let names = fields.iter().map(|f| &f.name);
    let reads = fields.iter().map(|field| {
        let ident = &field.ident;
        let wire = &field.name;
        let ty = &field.ty;
        let local = format_ident!("__field_{}", ident);
        let read = quote_spanned! {field.ty.span()=>
            let #local: #ty = __reader.value_field(#wire, __cx)?;
        };
        (read, quote! { #ident: #local, })
    });
    let (reads, inits): (Vec<_>, Vec<_>) = reads.unzip();
    quote! {
        const __FIELDS: &[&str] = &[#(#names),*];
        let __reader = ::techy::__private::FieldReader::new(__value, #what, __FIELDS)?;
        #(#reads)*
        ::core::result::Result::Ok(#constructor { #(#inits)* })
    }
}

// --- the rejections ---------------------------------------------------------------------

/// The shapes and attribute mistakes the derives refuse, each with an error at the
/// offending item. The accepted shapes are exercised inside techy (its in-crate derive
/// tests) and from a consumer crate (its integration tests).
#[cfg(test)]
mod tests {
    use syn::{parse_quote, DeriveInput};

    use super::{expand_deserializable_value, expand_serializable_value};

    /// The error text of both derives on `input` (they share the model, so they share
    /// the rejection); asserts that both reject.
    fn rejection(input: DeriveInput) -> String {
        let ser = expand_serializable_value(input.clone()).expect_err("SerializableValue accepts");
        let de = expand_deserializable_value(input).expect_err("DeserializableValue accepts");
        let ser = ser.to_string();
        let de = de.to_string();
        assert_eq!(
            ser.replace("SerializableValue", "…"),
            de.replace("DeserializableValue", "…"),
            "the two derives disagree on the rejection"
        );
        ser
    }

    #[test]
    fn a_field_without_a_wire_name_is_refused() {
        let message = rejection(parse_quote! {
            struct S {
                #[serial(name = "a")]
                a: u32,
                b: u32,
            }
        });
        assert!(message.contains("every field needs `#[serial(name = \"…\")]`"), "{message}");
        assert!(message.starts_with("#[derive(SerializableValue)]"), "{message}");
    }

    #[test]
    fn a_variant_without_a_wire_name_is_refused() {
        let message = rejection(parse_quote! {
            enum E {
                #[serial(name = "a")]
                A,
                B(u32),
            }
        });
        assert!(message.contains("every variant needs `#[serial(name = \"…\")]`"), "{message}");
    }

    #[test]
    fn a_repeated_wire_name_is_refused() {
        let message = rejection(parse_quote! {
            struct S {
                #[serial(name = "same")]
                a: u32,
                #[serial(name = "same")]
                b: u32,
            }
        });
        assert_eq!(message, "wire name `same` is used by another field of the same type");
        let message = rejection(parse_quote! {
            enum E {
                #[serial(name = "same")]
                A,
                #[serial(name = "same")]
                B,
            }
        });
        assert_eq!(message, "wire name `same` is used by another variant of the same type");
    }

    #[test]
    fn an_empty_wire_name_is_refused() {
        let message = rejection(parse_quote! {
            struct S {
                #[serial(name = "")]
                a: u32,
            }
        });
        assert_eq!(message, "the wire name must not be empty");
    }

    #[test]
    fn a_generic_type_is_refused() {
        let message = rejection(parse_quote! {
            struct S<T> {
                #[serial(name = "a")]
                a: T,
            }
        });
        assert!(message.contains("does not support generic types"), "{message}");
        let message = rejection(parse_quote! {
            struct S where u32: Copy {
                #[serial(name = "a")]
                a: u32,
            }
        });
        assert!(message.contains("does not support `where` clauses"), "{message}");
    }

    #[test]
    fn a_tuple_struct_and_a_unit_struct_are_refused() {
        let message = rejection(parse_quote! {
            struct S(u32);
        });
        assert!(message.contains("supports structs with named fields only"), "{message}");
        let message = rejection(parse_quote! {
            struct S;
        });
        assert!(message.contains("supports structs with named fields only"), "{message}");
    }

    #[test]
    fn a_tuple_variant_with_several_fields_is_refused() {
        let message = rejection(parse_quote! {
            enum E {
                #[serial(name = "pair")]
                Pair(u32, u32),
            }
        });
        assert!(message.contains("use named fields for several values"), "{message}");
    }

    #[test]
    fn a_wire_name_on_a_newtype_payload_is_refused() {
        let message = rejection(parse_quote! {
            enum E {
                #[serial(name = "one")]
                One(#[serial(name = "inner")] u32),
            }
        });
        assert!(message.contains("carries no wire name of its own"), "{message}");
    }

    #[test]
    fn a_serial_attribute_on_the_type_is_refused() {
        let message = rejection(parse_quote! {
            #[serial(name = "s")]
            struct S {
                #[serial(name = "a")]
                a: u32,
            }
        });
        assert!(message.contains("takes no `serial` attribute on the type itself"), "{message}");
    }

    #[test]
    fn an_unknown_serial_key_is_refused() {
        let message = rejection(parse_quote! {
            struct S {
                #[serial(rename = "a")]
                a: u32,
            }
        });
        assert!(message.contains("unknown `serial` key; expected `name`"), "{message}");
    }

    #[test]
    fn a_union_and_an_empty_enum_are_refused() {
        let message = rejection(parse_quote! {
            union U {
                a: u32,
            }
        });
        assert!(message.contains("supports structs and enums only"), "{message}");
        let message = rejection(parse_quote! {
            enum E {}
        });
        assert!(message.contains("requires at least one variant"), "{message}");
    }

    /// The accepted shapes expand (the generated code is compiled by techy's own
    /// tests); the expansion names the traits by their public paths and the helpers
    /// through `__private`, never `crate::`.
    #[test]
    fn accepted_shapes_expand_with_the_public_paths() {
        let input: DeriveInput = parse_quote! {
            enum E {
                #[serial(name = "unit")]
                Unit,
                #[serial(name = "one")]
                One(u32),
                #[serial(name = "many")]
                Many {
                    #[serial(name = "a")]
                    a: u32,
                    #[serial(name = "b")]
                    b: Option<String>,
                },
            }
        };
        let ser = expand_serializable_value(input.clone()).expect("expands").to_string();
        let de = expand_deserializable_value(input).expect("expands").to_string();
        for code in [&ser, &de] {
            assert!(code.contains(":: techy :: core :: Lang"), "{code}");
            assert!(code.contains(":: techy :: serialize :: SerializableLang"), "{code}");
            assert!(code.contains(":: techy :: __private ::"), "{code}");
            assert!(!code.contains("crate ::"), "{code}");
        }
        assert!(ser.contains(":: techy :: serialize :: SerializableValue < __L >"), "{ser}");
        assert!(de.contains(":: techy :: serialize :: DeserializableValue < __L >"), "{de}");
        assert!(de.contains("\"E::Many\""), "{de}");
    }
}
