//! Generation for the configuration readers
//!
//! When reading the configuration, we want to see default values if nothing has
//! been configured
use std::iter;
use std::iter::once;

use itertools::Itertools;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;
use syn::parse_quote;
use syn::parse_quote_spanned;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::Token;

use crate::error::extract_type_from_result;
use crate::input::ConfigurableField;
use crate::input::FieldDefault;
use crate::input::FieldOrGroup;
use crate::namegen::IdGenerator;
use crate::namegen::SequentialIdGenerator;
use crate::optional_error::OptionalError;
use crate::prefixed_type_name;

pub fn try_generate(
    root_name: proc_macro2::Ident,
    items: &[FieldOrGroup],
    doc_comment: &str,
) -> syn::Result<TokenStream> {
    let structs = generate_structs(&root_name, items, Vec::new(), doc_comment)?;
    let conversions = generate_conversions(&root_name, items, vec![], items)?;
    Ok(quote! {
        #structs
        #conversions
    })
}

fn generate_structs(
    name: &proc_macro2::Ident,
    items: &[FieldOrGroup],
    parents: Vec<PathItem>,
    doc_comment: &str,
) -> syn::Result<TokenStream> {
    let mut idents = Vec::new();
    let mut tys = Vec::<syn::Type>::new();
    let mut sub_readers = Vec::new();
    let mut attrs: Vec<Vec<syn::Attribute>> = Vec::new();
    let mut lazy_readers = Vec::new();
    let mut vis: Vec<syn::Visibility> = Vec::new();

    for item in items {
        match item {
            FieldOrGroup::Field(field) => {
                let ty = field.ty();
                attrs.push(field.attrs().to_vec());
                idents.push(field.ident());
                if field.is_optional() {
                    tys.push(parse_quote_spanned!(ty.span()=> OptionalConfig<#ty>));
                } else if let Some(field) = field.read_only() {
                    let name = field.lazy_reader_name(&parents);
                    tys.push(parse_quote_spanned!(field.ty.span()=> #name));
                    lazy_readers.push((name, &field.ty, &field.readonly.function));
                } else {
                    tys.push(ty.to_owned());
                }
                sub_readers.push(None);
                vis.push(match field.reader().private {
                    true => parse_quote!(),
                    false => parse_quote!(pub),
                });
            }
            FieldOrGroup::Multi(group) if !group.reader.skip => {
                let sub_reader_name = prefixed_type_name(name, group);
                idents.push(&group.ident);
                tys.push(parse_quote_spanned!(group.ident.span()=> MultiReader<#sub_reader_name>));
                let mut parents = parents.clone();
                parents.push(PathItem::Static(group.ident.clone()));
                parents.push(PathItem::Dynamic(group.ident.span()));
                sub_readers.push(Some(generate_structs(
                    &sub_reader_name,
                    &group.contents,
                    parents,
                    "",
                )?));
                attrs.push(group.attrs.to_vec());
                vis.push(match group.reader.private {
                    true => parse_quote!(),
                    false => parse_quote!(pub),
                });
            }
            FieldOrGroup::Group(group) if !group.reader.skip => {
                let sub_reader_name = prefixed_type_name(name, group);
                idents.push(&group.ident);
                tys.push(parse_quote_spanned!(group.ident.span()=> #sub_reader_name));
                let mut parents = parents.clone();
                parents.push(PathItem::Static(group.ident.clone()));
                sub_readers.push(Some(generate_structs(
                    &sub_reader_name,
                    &group.contents,
                    parents,
                    "",
                )?));
                attrs.push(group.attrs.to_vec());
                vis.push(match group.reader.private {
                    true => parse_quote!(),
                    false => parse_quote!(pub),
                });
            }
            FieldOrGroup::Group(_) => {
                // Explicitly skipped using `#[tedge_config(reader(skip))]`
            }
            FieldOrGroup::Multi(_) => {
                // Explicitly skipped using `#[tedge_config(reader(skip))]`
            }
        }
    }

    let lazy_reader_impls = lazy_readers
        .iter()
        .map(|(name, ty, function)| -> syn::ItemImpl {
            if let Some((ok, err)) = extract_type_from_result(ty) {
                parse_quote_spanned! {name.span()=>
                    impl #name {
                        // TODO don't just guess we're called tedgeconfigreader
                        pub fn try_read(&self, reader: &TEdgeConfigReader) -> Result<&#ok, #err> {
                            self.0.get_or_try_init(|| #function(reader))
                        }
                    }
                }
            } else {
                parse_quote_spanned! {name.span()=>
                    impl #name {
                        // TODO don't just guess we're called tedgeconfigreader
                        pub fn read(&self, reader: &TEdgeConfigReader) -> &#ty {
                            self.0.get_or_init(|| #function(reader))
                        }
                    }
                }
            }
        });

    let (lr_names, lr_tys): (Vec<_>, Vec<_>) = lazy_readers
        .iter()
        .map(|(name, ty, _)| match extract_type_from_result(ty) {
            Some((ok, _err)) => (name, ok),
            None => (name, *ty),
        })
        .unzip();

    let doc_comment_attr =
        (!doc_comment.is_empty()).then(|| quote_spanned!(name.span()=> #[doc = #doc_comment]));
    Ok(quote_spanned! {name.span()=>
        #[derive(::doku::Document, ::serde::Serialize, Debug, Clone)]
        #[non_exhaustive]
        #doc_comment_attr
        pub struct #name {
            #(
                #(#attrs)*
                #vis #idents: #tys,
            )*
        }

        #(
            #[derive(::serde::Serialize, Clone, Debug, Default)]
            #[serde(into = "()")]
            pub struct #lr_names(::once_cell::sync::OnceCell<#lr_tys>);

            impl From<#lr_names> for () {
                fn from(_: #lr_names) {}
            }

            #lazy_reader_impls
        )*

        #(#sub_readers)*
    })
}

fn find_field<'a>(
    mut fields: &'a [FieldOrGroup],
    key: &Punctuated<syn::Ident, Token![.]>,
) -> syn::Result<&'a ConfigurableField> {
    let mut current_field = None;
    for (i, segment) in key.iter().enumerate() {
        let target = fields
            .iter()
            .find(|field| field.is_called(segment))
            .ok_or_else(|| {
                syn::Error::new(
                    segment.span(),
                    format!(
                        "no field named `{segment}` {}",
                        current_field.map_or_else(
                            || "at top level of configuration".to_owned(),
                            |field: &FieldOrGroup| format!("in {}", field.ident())
                        )
                    ),
                )
            })?;

        let is_last_segment = i == key.len() - 1;
        match target {
            FieldOrGroup::Group(group) | FieldOrGroup::Multi(group) => fields = &group.contents,
            FieldOrGroup::Field(_) if is_last_segment => (),
            _ => {
                let string_path = key.iter().map(<_>::to_string).collect::<Vec<_>>();
                let (successful_segments, subfields) = string_path.split_at(i + 1);
                let successful_segments = successful_segments.join(".");
                let subfields = subfields.join(".");
                return Err(syn::Error::new(
                    segment.span(),
                    format!("cannot access `{subfields}` because `{successful_segments}` is a configuration field, not a group"),
                ));
            }
        };
        current_field = Some(target);
    }

    match current_field {
        // TODO test this appears
        None => Err(syn::Error::new(key.span(), "key is empty")),
        Some(FieldOrGroup::Group(_) | FieldOrGroup::Multi(_)) => Err(syn::Error::new(
            key.span(),
            // TODO test this too
            "path points to a group of fields, not a single field",
        )),
        Some(FieldOrGroup::Field(f)) => Ok(f),
    }
}

#[derive(Debug, Clone)]
/// Part of a path in the DTO
pub enum PathItem {
    /// A static field e.g. `c8y` or `topic_prefix`
    Static(syn::Ident),
    /// A dynamic field that will be replaced by `.try_get(key0)` when reading the field
    Dynamic(Span),
}

impl PathItem {
    pub fn as_static(&self) -> Option<&syn::Ident> {
        match self {
            Self::Static(s) => Some(s),
            Self::Dynamic(_) => None,
        }
    }
}

fn read_field(parents: &[PathItem]) -> impl Iterator<Item = TokenStream> + '_ {
    let mut id_gen = SequentialIdGenerator::default();
    let mut parent_key = String::new();
    parents.iter().map(move |parent| match parent {
        PathItem::Static(name) => {
            parent_key += &name.to_string();
            quote!(#name)
        }
        PathItem::Dynamic(span) => {
            let id = id_gen.next_id(*span);
            quote_spanned!(*span=> try_get(#id, #parent_key).unwrap())
        }
    })
}

fn reader_value_for_field<'a>(
    field: &'a ConfigurableField,
    parents: &[PathItem],
    root_fields: &[FieldOrGroup],
    mut observed_keys: Vec<&'a Punctuated<syn::Ident, Token![.]>>,
) -> syn::Result<TokenStream> {
    let name = field.ident();
    Ok(match field {
        ConfigurableField::ReadWrite(field) => {
            let key: syn::Expr = if parents.iter().all(|p| p.as_static().is_some()) {
                #[allow(unstable_name_collisions)]
                let key_str = parents
                    .iter()
                    .map(|p| match p {
                        PathItem::Static(p) => p.to_string(),
                        PathItem::Dynamic(_) => {
                            unreachable!("all pathitems are static in this if branch")
                        }
                    })
                    .chain(iter::once(name.to_string()))
                    .intersperse(".".to_owned())
                    .collect::<String>();
                parse_quote!(#key_str.into())
            } else {
                let mut id_gen = SequentialIdGenerator::default();
                let elems = parents
                    .iter()
                    .map::<syn::Expr, _>(|p| match p {
                        PathItem::Static(p) => {
                            let p_str = p.to_string();
                            parse_quote!(Some(#p_str))
                        }
                        PathItem::Dynamic(span) => {
                            let ident = id_gen.next_id(*span);
                            parse_quote!(#ident)
                        }
                    })
                    .chain({
                        let name = name.to_string();
                        iter::once(parse_quote!(Some(#name)))
                    });
                parse_quote!([#(#elems),*].into_iter().filter_map(|id| id).collect::<Vec<_>>().join(".").into())
            };
            let read_path = read_field(parents);
            match &field.default {
                FieldDefault::None => quote! {
                    match &dto.#(#read_path).*.#name {
                        None => OptionalConfig::Empty(#key),
                        Some(value) => OptionalConfig::Present { value: value.clone(), key: #key },
                    }
                },
                FieldDefault::FromKey(key) if observed_keys.contains(&key) => {
                    let string_paths = observed_keys
                        .iter()
                        .map(|path| {
                            path.iter()
                                .map(<_>::to_string)
                                .collect::<Vec<_>>()
                                .join(".")
                        })
                        .collect::<Vec<_>>();
                    let error =
                        format!("this path's default is part of a cycle ({string_paths:?})");
                    // Safe to unwrap the error since observed_paths.len() >= 1
                    return Err(observed_keys
                        .into_iter()
                        .map(|path| syn::Error::new(path.span(), &error))
                        .fold(OptionalError::default(), |mut errors, error| {
                            errors.combine(error);
                            errors
                        })
                        .take()
                        .unwrap());
                }
                FieldDefault::FromKey(default_key) | FieldDefault::FromOptionalKey(default_key) => {
                    observed_keys.push(default_key);
                    let default = reader_value_for_field(
                        find_field(root_fields, default_key)?,
                        &parents_for(default_key, parents, root_fields)?,
                        root_fields,
                        observed_keys,
                    )?;

                    let (default, value) =
                        if matches!(&field.default, FieldDefault::FromOptionalKey(_)) {
                            (
                                quote!(#default.map(|v| v.into())),
                                quote!(OptionalConfig::Present { value: value.clone(), key: #key }),
                            )
                        } else {
                            (quote!(#default.into()), quote!(value.clone()))
                        };

                    quote_spanned! {name.span()=>
                        match &dto.#(#read_path).*.#name {
                            Some(value) => #value,
                            None => #default,
                        }
                    }
                }
                FieldDefault::Function(function) => quote_spanned! {function.span()=>
                    match &dto.#(#read_path).*.#name {
                        None => TEdgeConfigDefault::<TEdgeConfigDto, _>::call(#function, dto, location),
                        Some(value) => value.clone(),
                    }
                },
                FieldDefault::Value(default) => quote_spanned! {name.span()=>
                    match &dto.#(#read_path).*.#name {
                        None => #default.into(),
                        Some(value) => value.clone(),
                    }
                },
                FieldDefault::Variable(default) => quote_spanned! {name.span()=>
                    match &dto.#(#read_path).*.#name {
                        None => #default.into(),
                        Some(value) => value.clone(),
                    }
                },
                FieldDefault::FromStr(default) => quote_spanned! {name.span()=>
                    match &dto.#(#read_path).*.#name {
                        None => #default.parse().unwrap(),
                        Some(value) => value.clone(),
                    }
                },
            }
        }
        ConfigurableField::ReadOnly(field) => {
            let name = field.lazy_reader_name(parents);
            quote! {
                #name::default()
            }
        }
    })
}

/// Generates the list of parent keys for the given tedge config key
///
/// This cross-correlates the provided key with the current key's parents,
/// so keys with profiles will work.
///
/// For example, in the case
///
/// ```no_compile
/// define_tedge_config! {
///     #[tedge_config(multi)]
///     c8y: {
///         url: String,
///
///         #[tedge_config(default(from_optional_key = "c8y.url"))]
///         http: String,
///     }
/// }
/// ```
///
/// The parents used in the default value for `c8y.*.http` will be equivalent `c8y.*.url`.
/// This means the c8y.url value used for c8y.http will use the same profile as the relevant
/// c8y.http.
fn parents_for(
    key: &Punctuated<syn::Ident, Token![.]>,
    parents: &[PathItem],
    root_fields: &[FieldOrGroup],
) -> syn::Result<Vec<PathItem>> {
    // Trace through the key to make sure we pick up dynamic paths as we expect
    // This allows e.g. c8y.http to have a default value of c8y.url, and we will pass in key0, key1 as expected
    // If we hit multi fields that aren't also parents of the current key, this is an error,
    // as there isn't a sensible resolution to this
    let mut parents = parents.iter().peekable();
    let mut fields = root_fields;
    let mut new_parents = vec![];
    for (i, field) in key.iter().take(key.len() - 1).enumerate() {
        new_parents.push(PathItem::Static(field.to_owned()));
        if let Some(PathItem::Static(s)) = parents.next() {
            if field == s {
                if let Some(PathItem::Dynamic(span)) = parents.peek() {
                    parents.next();
                    new_parents.push(PathItem::Dynamic(*span));
                }
            } else {
                // This key has diverged from the currrent key's parents, so empty the list
                // This will prevent unwanted matches that aren't genuine
                while parents.next().is_some() {}
            }
        }
        let root_field = fields.iter().find(|fog| fog.ident() == field).unwrap();
        match root_field {
            FieldOrGroup::Multi(m) => {
                if matches!(new_parents.last().unwrap(), PathItem::Dynamic(_)) {
                    fields = &m.contents;
                } else {
                    let multi_key = key
                        .iter()
                        .take(i + 1)
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join(".");
                    return Err(syn::Error::new(
                        key.span(),
                        format!("{multi_key} is a multi-value field"),
                    ));
                }
            }
            FieldOrGroup::Group(g) => {
                fields = &g.contents;
            }
            FieldOrGroup::Field(_) => {}
        }
    }
    Ok(new_parents)
}

/// Generate the conversion methods from DTOs to Readers
fn generate_conversions(
    name: &proc_macro2::Ident,
    items: &[FieldOrGroup],
    parents: Vec<PathItem>,
    root_fields: &[FieldOrGroup],
) -> syn::Result<TokenStream> {
    let mut field_conversions = Vec::new();
    let mut rest = Vec::new();
    let mut id_gen = SequentialIdGenerator::default();
    let extra_args: Vec<_> = parents
        .iter()
        .filter_map(|path_item| match path_item {
            PathItem::Static(_) => None,
            PathItem::Dynamic(span) => {
                let id = id_gen.next_id(*span);
                Some(quote_spanned!(*span=> #id: Option<&str>))
            }
        })
        .collect();

    for item in items {
        match item {
            FieldOrGroup::Field(field) => {
                let name = field.ident();
                let value = reader_value_for_field(field, &parents, root_fields, Vec::new())?;
                field_conversions.push(quote!(#name: #value));
            }
            FieldOrGroup::Group(group) if !group.reader.skip => {
                let sub_reader_name = prefixed_type_name(name, group);
                let name = &group.ident;

                let mut parents = parents.clone();
                parents.push(PathItem::Static(group.ident.clone()));
                let mut id_gen = SequentialIdGenerator::default();
                let extra_call_args: Vec<_> = parents
                    .iter()
                    .filter_map(|path_item| match path_item {
                        PathItem::Static(_) => None,
                        PathItem::Dynamic(span) => Some(id_gen.next_id(*span)),
                    })
                    .collect();
                field_conversions.push(
                    quote!(#name: #sub_reader_name::from_dto(dto, location, #(#extra_call_args),*)),
                );
                let sub_conversions =
                    generate_conversions(&sub_reader_name, &group.contents, parents, root_fields)?;
                rest.push(sub_conversions);
            }
            FieldOrGroup::Multi(group) if !group.reader.skip => {
                let sub_reader_name = prefixed_type_name(name, group);
                let name = &group.ident;

                let new_arg = PathItem::Dynamic(group.ident.span());
                let mut id_gen = SequentialIdGenerator::default();
                let extra_call_args: Vec<_> = parents
                    .iter()
                    .chain(once(&new_arg))
                    .filter_map(|path_item| match path_item {
                        PathItem::Static(_) => None,
                        PathItem::Dynamic(span) => Some(id_gen.next_id(*span)),
                    })
                    .collect();
                let mut parents = parents.clone();
                parents.push(PathItem::Static(group.ident.clone()));
                let read_path = read_field(&parents);
                #[allow(unstable_name_collisions)]
                let parent_key = parents
                    .iter()
                    .filter_map(|p| match p {
                        PathItem::Static(s) => Some(s.to_string()),
                        _ => None,
                    })
                    .intersperse(".".to_owned())
                    .collect::<String>();
                let new_arg2 = extra_call_args.last().unwrap().clone();
                field_conversions.push(quote!(#name: dto.#(#read_path).*.map_keys(|#new_arg2| #sub_reader_name::from_dto(dto, location, #(#extra_call_args),*), #parent_key)));
                parents.push(new_arg);
                let sub_conversions =
                    generate_conversions(&sub_reader_name, &group.contents, parents, root_fields)?;
                rest.push(sub_conversions);
            }
            FieldOrGroup::Group(_) | FieldOrGroup::Multi(_) => {
                // Skipped
            }
        }
    }

    Ok(quote! {
        impl #name {
            #[allow(unused, clippy::clone_on_copy, clippy::useless_conversion)]
            #[automatically_derived]
            /// Converts the provided [TEdgeConfigDto] into a reader
            pub fn from_dto(dto: &TEdgeConfigDto, location: &TEdgeConfigLocation, #(#extra_args,)*) -> Self {
                Self {
                    #(#field_conversions),*
                }
            }
        }

        #(#rest)*
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn from_optional_key_reuses_multi_fields() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            c8y: {
                url: String,

                #[tedge_config(default(from_optional_key = "c8y.url"))]
                http: String,
            }
        );
        let FieldOrGroup::Multi(m) = &input.groups[0] else {
            unreachable!()
        };
        let http = m.contents[1].field().unwrap();
        let actual = reader_value_for_field(
            http,
            &[
                PathItem::Static(parse_quote!(c8y)),
                PathItem::Dynamic(Span::call_site()),
            ],
            &input.groups,
            vec![],
        )
        .unwrap();
        let actual: syn::File = parse_quote!(fn dummy() { #actual });
        let c8y_http_key = quote! {
            [Some("c8y"), key0, Some("http")]
                .into_iter()
                .filter_map(|id| id)
                .collect::<Vec<_>>()
                .join(".")
                .into()
        };
        let c8y_url_key = quote! {
            [Some("c8y"), key0, Some("url")]
                .into_iter()
                .filter_map(|id| id)
                .collect::<Vec<_>>()
                .join(".")
                .into()
        };
        let expected: syn::Expr = parse_quote!(match &dto.c8y.try_get(key0, "c8y").unwrap().http {
            Some(value) => {
                OptionalConfig::Present {
                    value: value.clone(),
                    key: #c8y_http_key,
                }
            }
            None => {
                match &dto.c8y.try_get(key0, "c8y").unwrap().url {
                    None => OptionalConfig::Empty(#c8y_url_key),
                    Some(value) => OptionalConfig::Present {
                        value: value.clone(),
                        key: #c8y_url_key,
                    },
                }
                .map(|v| v.into())
            }
        });
        let expected = parse_quote!(fn dummy() { #expected });
        pretty_assertions::assert_eq!(
            prettyplease::unparse(&actual),
            prettyplease::unparse(&expected),
        )
    }

    #[test]
    fn from_optional_key_returns_error_with_invalid_multi() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            c8y: {
                url: String,
            },
            az: {
                // We can't derive this from c8y.url, as we don't have a profile to select from c8y
                #[tedge_config(default(from_optional_key = "c8y.url"))]
                url: String,
            }
        );

        let FieldOrGroup::Group(g) = &input.groups[1] else {
            unreachable!()
        };
        let az_url = g.contents[0].field().unwrap();
        let error = reader_value_for_field(
            az_url,
            &[PathItem::Static(parse_quote!(az))],
            &input.groups,
            vec![],
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "c8y is a multi-value field");
    }

    #[test]
    fn generate_conversions_passes_profile_keys_to_conversions_for_groups() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            c8y: {
                smartrest: {
                    templates: TemplatesSet,
                }
            },
        );
        let actual = generate_conversions(
            &parse_quote!(TEdgeConfigReader),
            &input.groups,
            Vec::new(),
            &input.groups,
        )
        .unwrap();
        let file: syn::File = syn::parse2(actual).unwrap();
        let r#impl = file
            .items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Impl(i) if i.self_ty == parse_quote!(TEdgeConfigReaderC8y) => Some(i),
                _ => None,
            })
            .unwrap();

        let expected = parse_quote! {
            impl TEdgeConfigReaderC8y {
                #[allow(unused, clippy::clone_on_copy, clippy::useless_conversion)]
                #[automatically_derived]
                /// Converts the provided [TEdgeConfigDto] into a reader
                pub fn from_dto(
                    dto: &TEdgeConfigDto,
                    location: &TEdgeConfigLocation,
                    key0: Option<&str>,
                ) -> Self {
                    Self {
                        smartrest: TEdgeConfigReaderC8ySmartrest::from_dto(dto, location, key0)
                    }
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#r#impl)),
            prettyplease::unparse(&expected)
        )
    }
}
