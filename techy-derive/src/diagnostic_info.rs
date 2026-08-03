//! Expansion of `#[derive(DiagnosticInfo)]` (see the macro doc in lib.rs).

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Field, Fields, Ident, LitStr};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    crate::ensure_no_generics(&input.generics, "DiagnosticInfo")?;

    let name = &input.ident;
    let fields: Vec<&Field> = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => named.named.iter().collect(),
            Fields::Unit => Vec::new(),
            Fields::Unnamed(unnamed) => {
                return Err(syn::Error::new_spanned(
                    unnamed,
                    "#[derive(DiagnosticInfo)] requires named fields (or a unit struct): \
                     field names are the payload's serialization keys \
                     (DESIGN_RATIONALE.md [§dd-dr:errors])",
                ));
            }
        },
        Data::Enum(data) => {
            return Err(syn::Error::new(
                data.enum_token.span,
                "#[derive(DiagnosticInfo)] supports structs only: a condition is one \
                 plain data struct (DESIGN_RATIONALE.md [§dd-dr:errors])",
            ));
        }
        Data::Union(data) => {
            return Err(syn::Error::new(
                data.union_token.span,
                "#[derive(DiagnosticInfo)] supports structs only",
            ));
        }
    };

    // Raw identifiers would desync the serialization key and the message-interpolation
    // name from the spelled field name; no condition needs them, so reject up front.
    for field in &fields {
        let ident = field.ident.as_ref().expect("named field has an ident");
        if ident.to_string().starts_with("r#") {
            return Err(syn::Error::new(
                ident.span(),
                "#[derive(DiagnosticInfo)] does not support raw-identifier fields",
            ));
        }
    }

    let attrs = DiagnosticAttrs::parse(&input)?;

    let info_impl = expand_info_impl(name, &attrs.id, &fields);
    let display_impl = match &attrs.message {
        Some(message) => Some(expand_display_impl(name, message, &fields)?),
        None => None,
    };
    let constructor = if attrs.no_constructor {
        None
    } else {
        Some(expand_constructor(name, &fields))
    };

    Ok(quote! {
        #info_impl
        #display_impl
        #constructor
    })
}

/// The parsed `#[diagnostic(…)]` attribute: `id` (mandatory), `message` (optional),
/// `no_constructor` (flag).
struct DiagnosticAttrs {
    id: LitStr,
    message: Option<LitStr>,
    no_constructor: bool,
}

impl DiagnosticAttrs {
    fn parse(input: &DeriveInput) -> syn::Result<DiagnosticAttrs> {
        let mut id: Option<LitStr> = None;
        let mut message: Option<LitStr> = None;
        let mut no_constructor = false;

        for attr in &input.attrs {
            if !attr.path().is_ident("diagnostic") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("id") {
                    if id.is_some() {
                        return Err(meta.error("duplicate `id`"));
                    }
                    let lit: LitStr = meta.value()?.parse()?;
                    validate_identifier(&lit)?;
                    id = Some(lit);
                } else if meta.path.is_ident("message") {
                    if message.is_some() {
                        return Err(meta.error("duplicate `message`"));
                    }
                    message = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("no_constructor") {
                    if no_constructor {
                        return Err(meta.error("duplicate `no_constructor`"));
                    }
                    no_constructor = true;
                } else {
                    return Err(meta.error(
                        "unknown `diagnostic` key; expected `id`, `message`, or \
                         `no_constructor`",
                    ));
                }
                Ok(())
            })?;
        }

        let id = id.ok_or_else(|| {
            syn::Error::new(
                input.ident.span(),
                "missing `#[diagnostic(id = \"…\")]`: the wire identifier is mandatory \
                 and never derived from the type name (DESIGN_RATIONALE.md [§dd-dr:errors])",
            )
        })?;

        Ok(DiagnosticAttrs { id, message, no_constructor })
    }
}

/// The identifier is namespaced `<crate-or-lang>.<area>.<condition>`;
/// enforce the coarse shape, not the exact scheme.
fn validate_identifier(lit: &LitStr) -> syn::Result<()> {
    let value = lit.value();
    if value.is_empty()
        || !value.contains('.')
        || value.contains(char::is_whitespace)
        || value.starts_with('.')
        || value.ends_with('.')
    {
        return Err(syn::Error::new(
            lit.span(),
            "the identifier must be a namespaced dotted name like \
             `core.area.condition` (DESIGN_RATIONALE.md [§dd-dr:errors])",
        ));
    }
    Ok(())
}

/// `impl DiagnosticInfo`: the `IDENTIFIER` const and `serializable_data()` mapping
/// every field through `ToDiagnosticValue`, keyed by field name. Each conversion call
/// is spanned at the field's type, so a non-serializable field type reports at its
/// declaration.
fn expand_info_impl(name: &Ident, id: &LitStr, fields: &[&Field]) -> TokenStream {
    let body = if fields.is_empty() {
        quote! { ::techy::__private::DiagnosticValue::empty_map() }
    } else {
        let len = fields.len();
        let entries = fields.iter().map(|field| {
            let ident = field.ident.as_ref().expect("named field has an ident");
            let key = ident.to_string();
            quote_spanned! {field.ty.span()=>
                __data.push((
                    ::techy::__private::String::from(#key),
                    ::techy::__private::ToDiagnosticValue::to_diagnostic_value(&self.#ident),
                ));
            }
        });
        quote! {
            let mut __data = ::techy::__private::Vec::with_capacity(#len);
            #(#entries)*
            ::techy::__private::DiagnosticValue::Map(__data)
        }
    };

    quote! {
        #[automatically_derived]
        impl ::techy::__private::DiagnosticInfo for #name {
            const IDENTIFIER: &'static str = #id;

            fn serializable_data(&self) -> ::techy::__private::DiagnosticValue {
                #body
            }
        }
    }
}

/// `impl Display` from the message format string: `{field}` / `{field:spec}` are
/// emitted as explicit named `write!` arguments (no positional or implicit captures).
fn expand_display_impl(
    name: &Ident,
    message: &LitStr,
    fields: &[&Field],
) -> syn::Result<TokenStream> {
    let referenced = referenced_names(message)?;

    let field_names: Vec<String> = fields
        .iter()
        .map(|f| f.ident.as_ref().expect("named field has an ident").to_string())
        .collect();
    for reference in &referenced {
        if !field_names.contains(reference) {
            return Err(syn::Error::new(
                message.span(),
                format!(
                    "message references unknown field `{reference}`; {}",
                    if field_names.is_empty() {
                        "the struct has no fields".to_string()
                    } else {
                        format!("available fields: `{}`", field_names.join("`, `"))
                    }
                ),
            ));
        }
    }

    let args = referenced.iter().map(|reference| {
        let ident = Ident::new(reference, message.span());
        quote! { #ident = &self.#ident }
    });

    Ok(quote! {
        #[automatically_derived]
        impl ::core::fmt::Display for #name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::write!(f, #message #(, #args)*)
            }
        }
    })
}

/// The field names referenced by `{name}` / `{name:spec}` placeholders, in first-use
/// order, deduplicated. `{{`/`}}` escape; positional placeholders are rejected (the
/// message vocabulary is field names only).
fn referenced_names(message: &LitStr) -> syn::Result<Vec<String>> {
    let text = message.value();
    let err = |detail: &str| syn::Error::new(message.span(), detail);

    let mut names: Vec<String> = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    continue;
                }
                let mut name = String::new();
                let mut closed = false;
                for c2 in chars.by_ref() {
                    match c2 {
                        '}' => {
                            closed = true;
                            break;
                        }
                        ':' => {
                            // Format spec: skip to the closing brace (specs never
                            // contain braces; runtime width/precision use `name$`).
                            for c3 in chars.by_ref() {
                                if c3 == '}' {
                                    closed = true;
                                    break;
                                }
                            }
                            break;
                        }
                        other => name.push(other),
                    }
                }
                if !closed {
                    return Err(err("unclosed `{` in message"));
                }
                if name.is_empty() || name.chars().all(|c| c.is_ascii_digit()) {
                    return Err(err(
                        "message interpolation uses named fields only (`{field}`); \
                         positional `{}` / `{0}` placeholders are not supported",
                    ));
                }
                let valid_start =
                    name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
                let valid_rest =
                    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !valid_start || !valid_rest {
                    return Err(err(&format!(
                        "`{{{name}}}` is not a valid field reference"
                    )));
                }
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                } else {
                    return Err(err("unmatched `}` in message (use `}}` for a literal)"));
                }
            }
            _ => {}
        }
    }
    Ok(names)
}

/// The `new()` constructor: one `impl Into<FieldType>` parameter per field, in
/// declaration order — the companion of `#[non_exhaustive]`.
fn expand_constructor(name: &Ident, fields: &[&Field]) -> TokenStream {
    let params = fields.iter().map(|field| {
        let ident = field.ident.as_ref().expect("named field has an ident");
        let ty = &field.ty;
        quote! { #ident: impl ::core::convert::Into<#ty> }
    });
    let inits = fields.iter().map(|field| {
        let ident = field.ident.as_ref().expect("named field has an ident");
        quote! { #ident: ::core::convert::Into::into(#ident) }
    });
    let doc = format!("Creates a [`{name}`] condition.");

    quote! {
        impl #name {
            #[doc = #doc]
            #[allow(clippy::new_without_default)]
            pub fn new(#(#params),*) -> Self {
                Self { #(#inits),* }
            }
        }
    }
}
