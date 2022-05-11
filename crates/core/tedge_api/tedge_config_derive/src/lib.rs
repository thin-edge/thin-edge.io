use proc_macro::TokenStream as TS;
use proc_macro2::TokenStream;
use proc_macro_error::{abort, emit_error, proc_macro_error, OptionExt, ResultExt};
use quote::{quote, ToTokens, TokenStreamExt};
use syn::{
    parse_macro_input, Attribute, DeriveInput, Ident, Lit, LitStr, Meta, MetaNameValue, NestedMeta,
    Type,
};

#[derive(Debug)]
struct ConfigField<'q> {
    ident: &'q Ident,
    ty: &'q Type,
    docs: Option<Vec<LitStr>>,
}

#[derive(Debug)]
enum ConfigVariantKind<'q> {
    String(&'q Ident),
    Wrapped(&'q Ident, ConfigField<'q>),
    Struct(&'q Ident, Vec<ConfigField<'q>>),
}

#[derive(Debug)]
struct ConfigVariant<'q> {
    kind: ConfigVariantKind<'q>,
    docs: Option<Vec<LitStr>>,
}

#[derive(Debug)]
enum ConfigEnumKind {
    Tagged(LitStr),
    Untagged,
}

#[derive(Debug)]
enum ConfigQuoteKind<'q> {
    Wrapped(&'q Type),
    Struct(Vec<ConfigField<'q>>),
    Enum(ConfigEnumKind, Vec<ConfigVariant<'q>>),
}

#[derive(Debug)]
struct ConfigQuote<'q> {
    ident: &'q Ident,
    docs: Option<Vec<LitStr>>,
    kind: ConfigQuoteKind<'q>,
}

fn lit_strings_to_string_quoted(docs: &Option<Vec<LitStr>>) -> TokenStream {
    if let Some(docs) = docs {
        let docs = docs
            .iter()
            .map(|litstr| litstr.value().trim().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        quote!(Some(#docs))
    } else {
        quote!(None)
    }
}

fn extract_docs_from_attributes<'a>(
    attrs: impl Iterator<Item = &'a Attribute>,
) -> Option<Vec<LitStr>> {
    let attrs = attrs
        .filter_map(|attr| {
            if let Ok(Meta::NameValue(meta)) = attr.parse_meta() {
                if meta.path.is_ident("doc") {
                    if let Lit::Str(litstr) = meta.lit {
                        return Some(litstr);
                    }
                }
            }
            None
        })
        .collect::<Vec<_>>();

    if attrs.is_empty() {
        None
    } else {
        Some(attrs)
    }
}

impl<'q> ToTokens for ConfigQuote<'q> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ident_name = self.ident.to_string();
        let outer_docs = lit_strings_to_string_quoted(&self.docs);

        tokens.append_all(match &self.kind {
            ConfigQuoteKind::Wrapped(ty) => {
                quote! {
                    ::tedge_api::config::ConfigDescription::new(
                        ::std::string::String::from(#ident_name),
                        ::tedge_api::config::ConfigKind::Wrapped(
                            ::std::boxed::Box::new(<#ty as ::tedge_api::AsConfig>::as_config())
                        ),
                        #outer_docs
                    )
                }
            }
            ConfigQuoteKind::Struct(fields) => {
                let ident = fields.iter().map(|f| f.ident.to_string());
                let ty = fields.iter().map(|f| f.ty);
                let docs = fields.iter().map(|f| lit_strings_to_string_quoted(&f.docs));

                quote! {
                    ::tedge_api::config::ConfigDescription::new(
                        ::std::string::String::from(#ident_name),
                        ::tedge_api::config::ConfigKind::Struct(
                            vec![
                                #(
                                    (#ident, #docs, <#ty as ::tedge_api::AsConfig>::as_config())
                                ),*
                            ]
                        ),
                        #outer_docs
                    )
                }
            }
            ConfigQuoteKind::Enum(kind, variants) => {
                let kind = match kind {
                    ConfigEnumKind::Tagged(tag) => {
                        quote! {
                            ::tedge_api::config::ConfigEnumKind::Tagged(#tag)
                        }
                    }
                    ConfigEnumKind::Untagged => {
                        quote! {
                            ::tedge_api::config::ConfigEnumKind::Untagged
                        }
                    }
                };

                let variants = variants.iter().map(|var| {
                    let docs = lit_strings_to_string_quoted(&var.docs);
                    match &var.kind {
                        ConfigVariantKind::Wrapped(ident, ConfigField { ty, .. }) => {
                            // we ignore the above docs since the outer docs ar ethe important ones
                            // TODO: Emit an error if an inner type in a enum is annotated
                            let ident = ident.to_string();
                            quote!{
                                (
                                    #ident,
                                    #docs,
                                    ::tedge_api::config::EnumVariantRepresentation::Wrapped(
                                        std::boxed::Box::new(::tedge_api::config::ConfigDescription::new(
                                            ::std::string::String::from(#ident),
                                            ::tedge_api::config::ConfigKind::Wrapped(
                                                std::boxed::Box::new(<#ty as ::tedge_api::AsConfig>::as_config())
                                            ),
                                            None,
                                        ))
                                    )
                                )
                            }
                        }
                        ConfigVariantKind::Struct(ident, fields) => {
                            let ident = ident.to_string();
                            let idents = fields.iter().map(|f| f.ident.to_string());
                            let field_docs = fields.iter().map(|f| lit_strings_to_string_quoted(&f.docs));
                            let tys = fields.iter().map(|f| f.ty);

                            quote! {
                                (
                                    #ident,
                                    #docs,
                                    ::tedge_api::config::EnumVariantRepresentation::Wrapped(
                                        std::boxed::Box::new(::tedge_api::config::ConfigDescription::new(
                                            ::std::string::String::from(#ident),
                                            ::tedge_api::config::ConfigKind::Struct(
                                                vec![
                                                    #(
                                                        (#idents, #field_docs, <#tys as ::tedge_api::AsConfig>::as_config())
                                                     ),*
                                                ]
                                            ),
                                            None
                                        ))
                                    )
                                )
                            }
                        }
                        ConfigVariantKind::String(ident) => {
                            let ident = ident.to_string();
                            quote!{
                                (
                                    #ident,
                                    #docs,
                                    ::tedge_api::config::EnumVariantRepresentation::String(
                                        #ident
                                    )
                                )
                            }
                        }
                    }
                });

                quote! {
                    ::tedge_api::config::ConfigDescription::new(
                        ::std::string::String::from(#ident_name),
                        ::tedge_api::config::ConfigKind::Enum(
                            #kind,
                            vec![#(#variants),*]
                        ),
                        #outer_docs
                    )
                }
            }
        });
    }
}

#[proc_macro_derive(Config, attributes(config))]
#[proc_macro_error]
pub fn derive_config(input: TS) -> TS {
    let input = parse_macro_input!(input as DeriveInput);

    let ident = &input.ident;

    let config_desc_kind: ConfigQuoteKind = match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(fields) => ConfigQuoteKind::Struct(
                fields
                    .named
                    .iter()
                    .map(|f| ConfigField {
                        ident: &f.ident.as_ref().unwrap(),
                        ty: &f.ty,
                        docs: extract_docs_from_attributes(f.attrs.iter()),
                    })
                    .collect(),
            ),
            syn::Fields::Unnamed(fields) => {
                if fields.unnamed.len() != 1 {
                    abort!(
                        fields,
                        "Tuple structs should only contain a single variant."
                    )
                }
                ConfigQuoteKind::Wrapped(&fields.unnamed.first().unwrap().ty)
            }
            syn::Fields::Unit => abort!(
                ident,
                "Unit structs are not supported as they cannot be represented"
            ),
        },
        syn::Data::Enum(data) => {
            let enum_kind: ConfigEnumKind = {
                let potential_kind = input
                    .attrs
                    .iter()
                    .find(|attr| attr.path.is_ident("config"))
                    .unwrap_or_else(|| {
                        abort!(ident, "Enums need to specify what kind of tagging they use"; 
                               help = "Use #[config(untagged)] for untagged enums, and #[config(tag = \"type\")] for internally tagged variants. Other kinds are not supported.")
                    });

                macro_rules! abort_parse_enum_kind {
                    ($kind:expr) => {
                            abort!($kind, "Could not parse enum tag kind.";
                                   help = "Accepted kinds are #[config(untagged)] and #[config(tag = \"type\')].")
                    }
                }

                match potential_kind
                    .parse_meta()
                    .expect_or_abort("Could not parse #[config] meta attribute.")
                {
                    syn::Meta::Path(kind) => {
                        abort_parse_enum_kind!(kind)
                    }
                    syn::Meta::List(kind) => {
                        if kind.nested.len() != 1 {
                            abort_parse_enum_kind!(kind)
                        }

                        match kind.nested.first() {
                            Some(NestedMeta::Meta(Meta::NameValue(MetaNameValue {
                                path,
                                lit: Lit::Str(lit_str),
                                ..
                            }))) => {
                                if path.is_ident("tag") {
                                    ConfigEnumKind::Tagged(lit_str.clone())
                                } else {
                                    abort_parse_enum_kind!(kind)
                                }
                            }
                            Some(NestedMeta::Meta(Meta::Path(path))) => {
                                if path.is_ident("untagged") {
                                    ConfigEnumKind::Untagged
                                } else {
                                    abort_parse_enum_kind!(path)
                                }
                            }
                            _ => {
                                println!("Oh no!");
                                abort_parse_enum_kind!(kind)
                            }
                        }
                    }
                    syn::Meta::NameValue(kind) => abort!(
                        kind,
                        "The #[config] attribute cannot be used as a name-value attribute.";
                        help = "Maybe you meant #[config(tag = \"type\")] to describe that this enum has an internal tag?"
                    ),
                }
            };

            let variants = data
                .variants
                .iter()
                .map(|var| {
                    let kind = match &var.fields {
                        syn::Fields::Named(fields) => ConfigVariantKind::Struct(
                            &var.ident,
                            fields
                                .named
                                .iter()
                                .map(|f| ConfigField {
                                    ident: &f.ident.as_ref().unwrap(),
                                    ty: &f.ty,
                                    docs: extract_docs_from_attributes(f.attrs.iter()),
                                })
                                .collect(),
                        ),
                        syn::Fields::Unnamed(fields) => {
                            if fields.unnamed.len() != 1 {
                                abort!(
                                    fields,
                                    "Tuple structs should only contain a single variant."
                                )
                            }
                            ConfigVariantKind::Wrapped(
                                &var.ident,
                                ConfigField {
                                    ident: &var.ident,
                                    ty: &fields.unnamed.first().unwrap().ty,
                                    docs: extract_docs_from_attributes(var.attrs.iter()),
                                },
                            )
                        }
                        syn::Fields::Unit => ConfigVariantKind::String(&var.ident),
                    };
                    let docs = extract_docs_from_attributes(var.attrs.iter());
                    Some(ConfigVariant { kind, docs })
                })
                .collect::<Option<_>>();

            ConfigQuoteKind::Enum(
                enum_kind,
                variants.expect_or_abort("Enum contains invalid variants"),
            )
        }
        syn::Data::Union(_) => {
            abort!(
                ident,
                "Untagged unions are not supported. Consider using an enum instead."
            );
        }
    };

    let docs = extract_docs_from_attributes(input.attrs.iter());

    let config_desc = ConfigQuote {
        kind: config_desc_kind,
        docs,
        ident,
    };

    let expanded = quote! {
        impl ::tedge_api::config::AsConfig for #ident {
            fn as_config() -> ::tedge_api::config::ConfigDescription {
                #config_desc
            }
        }
    };

    TS::from(expanded)
}
