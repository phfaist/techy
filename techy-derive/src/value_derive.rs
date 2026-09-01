//! Expansion of the public `#[derive(SerializableValue)]` /
//! `#[derive(DeserializableValue)]` pair (see the macro docs in lib.rs): the impls of
//! techy's value capability traits for a plain-data struct or enum, with the
//! serialization context passed to every field's own conversion. The type model and
//! the attribute grammar are those of the internal wire derives
//! ([`parse_model`](crate::serial_value::parse_model)), plus the type-level
//! `#[serial(lang = …)]`; only the generated code differs — every field converts
//! through its own `SerializableValue<L>` / `DeserializableValue<L>` impl with the
//! context, and the generated impl is for every `L: Lang`, or for the one language
//! `lang` names.
//!
//! Generated code names the capability traits and the language bound by their
//! canonical public paths (`::techy::serialize::…`, `::techy::core::Lang`) — a
//! `#[doc(hidden)]` import path to a public trait makes semver tooling report the
//! trait as sealed — and everything else (contexts, errors, the value type, the field
//! and variant helpers) through `::techy::__private::…`. The impl's type parameter is
//! `__L` and its locals are `__`-prefixed: names user code is not expected to use; the
//! fields of a struct variant are bound to `__field_<name>` locals, never to their own
//! names, so a field called `__cx` or `__writer` cannot shadow the generated locals.

use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{DeriveInput, Ident};

use crate::serial_value::{parse_model, Model, NamedField, ParsedType, TypeAttributes, VariantKind};

const NO_GENERICS_REASON: &str = "a derived type is concrete; a value whose type depends on the \
                                  language is held as an already converted `SerialValue`, or the \
                                  language is named with `#[serial(lang = …)]`";

/// The language the generated impl is for: every language — a type parameter `__L`
/// bounded by `Lang`, the method bounded by `SerializableLang` as the trait's is — or
/// the one named by `#[serial(lang = …)]`: a concrete impl whose method carries no
/// bound of its own (an impl method may carry fewer bounds than the trait's, and the
/// context type in its signature already requires the language to be a
/// `SerializableLang`).
struct ImplLang {
    /// The impl's generic parameter list: `<__L: Lang>`, or nothing.
    generics: TokenStream,
    /// The language type in the trait and the context types: `__L`, or the named type.
    lang: TokenStream,
    /// The method's `where` clause: `where __L: SerializableLang`, or nothing.
    where_clause: TokenStream,
}

impl ImplLang {
    fn of(lang: Option<&syn::Type>) -> ImplLang {
        match lang {
            Some(ty) => ImplLang {
                generics: TokenStream::new(),
                lang: quote! { #ty },
                where_clause: TokenStream::new(),
            },
            None => ImplLang {
                generics: quote! { <__L: ::techy::core::Lang> },
                lang: quote! { __L },
                where_clause: quote! { where __L: ::techy::serialize::SerializableLang },
            },
        }
    }
}

/// The local a struct-variant field is bound to (and a struct field is read into).
fn field_local(ident: &Ident) -> Ident {
    format_ident!("__field_{}", ident)
}

// --- SerializableValue ------------------------------------------------------------------

pub(crate) fn expand_serializable_value(input: DeriveInput) -> syn::Result<TokenStream> {
    let ParsedType { model, lang } =
        parse_model(&input, "SerializableValue", NO_GENERICS_REASON, TypeAttributes::Lang)?;
    let ImplLang { generics, lang, where_clause } = ImplLang::of(lang.as_ref());
    let name = &input.ident;
    let body = match &model {
        Model::Struct(fields) => {
            let write = write_fields(fields, &lang, |field| quote! { &self.#field });
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
                            <#ty as ::techy::serialize::SerializableValue<#lang>>::serialize_value(
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
                        let bindings = fields.iter().map(|field| {
                            let ident = &field.ident;
                            let local = field_local(ident);
                            quote! { #ident: #local }
                        });
                        let write = write_fields(fields, &lang, |field| {
                            let local = field_local(field);
                            quote! { #local }
                        });
                        quote! {
                            Self::#ident { #(#bindings),* } => {
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
        impl #generics ::techy::serialize::SerializableValue<#lang> for #name {
            // A `__field_` local of a field whose own name starts with `_` is not
            // snake case.
            #[allow(non_snake_case)]
            fn serialize_value(
                &self,
                __cx: &mut ::techy::__private::SerializeContext<'_, #lang>,
            ) -> ::core::result::Result<
                ::techy::__private::SerialValue,
                ::techy::__private::SerializeError,
            >
            #where_clause
            {
                #body
            }
        }
    })
}

/// The statements declaring `__writer` and writing the named fields into it through
/// their `SerializableValue<lang>` impls with the context (the caller finishes the
/// writer — the `?`s propagate to the generated method directly, whose error type is
/// the helper's); `access` produces the expression yielding `&FieldType` for a field
/// ident. The explicit type arguments of the call put an unsatisfied-bound error at
/// the field type.
fn write_fields(
    fields: &[NamedField],
    lang: &TokenStream,
    access: impl Fn(&Ident) -> TokenStream,
) -> TokenStream {
    let len = fields.len();
    let writes = fields.iter().map(|field| {
        let wire = &field.name;
        let ty = &field.ty;
        let value = access(&field.ident);
        quote_spanned! {field.ty.span()=>
            __writer.value_field::<#lang, #ty>(#wire, #value, __cx)?;
        }
    });
    quote! {
        let mut __writer = ::techy::__private::FieldWriter::with_capacity(#len);
        #(#writes)*
    }
}

// --- DeserializableValue ----------------------------------------------------------------

pub(crate) fn expand_deserializable_value(input: DeriveInput) -> syn::Result<TokenStream> {
    let ParsedType { model, lang } =
        parse_model(&input, "DeserializableValue", NO_GENERICS_REASON, TypeAttributes::Lang)?;
    let ImplLang { generics, lang, where_clause } = ImplLang::of(lang.as_ref());
    let name = &input.ident;
    let type_name = name.to_string();
    let body = match &model {
        Model::Struct(fields) => read_fields(fields, &lang, &type_name, quote! { #name }),
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
                            <#ty as ::techy::serialize::DeserializableValue<#lang>>::deserialize_value(
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
                        let read = read_fields(fields, &lang, &what, quote! { Self::#ident });
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
        impl #generics ::techy::serialize::DeserializableValue<#lang> for #name {
            // A `__field_` local of a field whose own name starts with `_` is not
            // snake case.
            #[allow(non_snake_case)]
            fn deserialize_value(
                __value: &::techy::__private::SerialValue,
                __cx: &mut ::techy::__private::DeserializeContext<'_, #lang>,
            ) -> ::core::result::Result<Self, ::techy::__private::DeserializeError>
            #where_clause
            {
                #body
            }
        }
    })
}

/// Reads named fields from `__value` through a `FieldReader` and their
/// `DeserializableValue<lang>` impls with the context, and builds `constructor { … }`;
/// `what` names the type (or variant) in the shape error. The explicit type arguments
/// of the call put an unsatisfied-bound error at the field type.
fn read_fields(
    fields: &[NamedField],
    lang: &TokenStream,
    what: &str,
    constructor: TokenStream,
) -> TokenStream {
    let names = fields.iter().map(|f| &f.name);
    let reads = fields.iter().map(|field| {
        let ident = &field.ident;
        let wire = &field.name;
        let ty = &field.ty;
        let local = field_local(ident);
        let read = quote_spanned! {field.ty.span()=>
            let #local: #ty = __reader.value_field::<#lang, #ty>(#wire, __cx)?;
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
/// offending item, and the shape of the accepted expansions. The generated code itself
/// is compiled and run inside techy (its in-crate derive tests) and from a consumer
/// crate (its integration tests).
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

    /// A generic type is refused with the public derives' own reason, which names the
    /// two ways out.
    #[test]
    fn a_generic_type_is_refused() {
        let message = rejection(parse_quote! {
            struct S<T> {
                #[serial(name = "a")]
                a: T,
            }
        });
        assert!(message.contains("does not support generic types"), "{message}");
        assert!(message.contains("a derived type is concrete"), "{message}");
        assert!(message.contains("`#[serial(lang = …)]`"), "{message}");
        assert!(!message.contains("wire structs"), "{message}");
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
    fn a_name_on_the_type_is_refused_pointing_at_lang() {
        let message = rejection(parse_quote! {
            #[serial(name = "s")]
            struct S {
                #[serial(name = "a")]
                a: u32,
            }
        });
        assert_eq!(
            message,
            "`name` goes on fields and variants; the type-level attribute takes `lang = …` only"
        );
    }

    #[test]
    fn an_unknown_key_on_the_type_is_refused() {
        let message = rejection(parse_quote! {
            #[serial(rename = "s")]
            struct S {
                #[serial(name = "a")]
                a: u32,
            }
        });
        assert_eq!(message, "unknown `serial` key on the type; expected `lang`");
    }

    #[test]
    fn a_second_lang_is_refused() {
        let message = rejection(parse_quote! {
            #[serial(lang = A)]
            #[serial(lang = B)]
            struct S {
                #[serial(name = "a")]
                a: u32,
            }
        });
        assert_eq!(message, "duplicate `lang`");
        let message = rejection(parse_quote! {
            #[serial(lang = A, lang = B)]
            struct S {
                #[serial(name = "a")]
                a: u32,
            }
        });
        assert_eq!(message, "duplicate `lang`");
    }

    #[test]
    fn a_lang_without_a_type_is_refused() {
        let message = rejection(parse_quote! {
            #[serial(lang)]
            struct S {
                #[serial(name = "a")]
                a: u32,
            }
        });
        assert!(message.contains("expected `=`"), "{message}");
        let message = rejection(parse_quote! {
            #[serial(lang = 3)]
            struct S {
                #[serial(name = "a")]
                a: u32,
            }
        });
        assert!(message.contains("expected"), "{message}");
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
    /// through `__private`, never `crate::`; without `lang`, the impl is for `__L`, and
    /// a struct variant's fields are bound to `__field_` locals.
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
            assert!(code.contains("impl < __L : :: techy :: core :: Lang >"), "{code}");
            assert!(code.contains("where __L : :: techy :: serialize :: SerializableLang"), "{code}");
            assert!(code.contains(":: techy :: __private ::"), "{code}");
            assert!(!code.contains("crate ::"), "{code}");
        }
        assert!(ser.contains(":: techy :: serialize :: SerializableValue < __L >"), "{ser}");
        assert!(ser.contains("Self :: Many { a : __field_a , b : __field_b }"), "{ser}");
        assert!(ser.contains("value_field :: < __L , u32 >"), "{ser}");
        assert!(de.contains(":: techy :: serialize :: DeserializableValue < __L >"), "{de}");
        assert!(de.contains("\"E::Many\""), "{de}");
    }

    /// With `#[serial(lang = …)]` the impl is for that language alone: no type
    /// parameter, no `where` clause, the named type in the trait, the contexts, and
    /// the field calls.
    #[test]
    fn a_named_language_gives_a_concrete_impl() {
        let input: DeriveInput = parse_quote! {
            #[serial(lang = my::Lang)]
            struct S {
                #[serial(name = "a")]
                a: u32,
            }
        };
        let ser = expand_serializable_value(input.clone()).expect("expands").to_string();
        let de = expand_deserializable_value(input).expect("expands").to_string();
        assert!(ser.contains("impl :: techy :: serialize :: SerializableValue < my :: Lang > for S"), "{ser}");
        assert!(ser.contains("SerializeContext < '_ , my :: Lang >"), "{ser}");
        assert!(ser.contains("value_field :: < my :: Lang , u32 >"), "{ser}");
        assert!(de.contains("impl :: techy :: serialize :: DeserializableValue < my :: Lang > for S"), "{de}");
        assert!(de.contains("DeserializeContext < '_ , my :: Lang >"), "{de}");
        assert!(de.contains("value_field :: < my :: Lang , u32 >"), "{de}");
        for code in [&ser, &de] {
            assert!(!code.contains("__L"), "{code}");
            assert!(!code.contains("where"), "{code}");
        }
    }
}
