//! Generation for the configuration readers
//!
//! When reading the configuration, we want to see default values if nothing has
//! been configured
use std::iter::once;

use heck::ToPascalCase;
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
                if let Some((function, rw_field)) = field.reader_function() {
                    let name = rw_field.lazy_reader_name(&parents);
                    let parent_ty = rw_field.parent_name(&parents);
                    tys.push(parse_quote_spanned!(ty.span()=> #name));
                    let dto_ty: syn::Type = match extract_type_from_result(&rw_field.ty) {
                        Some((ok, _err)) => parse_quote!(OptionalConfig<#ok>),
                        None => {
                            let ty = &rw_field.ty;
                            parse_quote!(OptionalConfig<#ty>)
                        }
                    };
                    lazy_readers.push((
                        name,
                        &rw_field.ty,
                        function,
                        parent_ty,
                        rw_field.ident.clone(),
                        dto_ty.clone(),
                        visibility(field),
                    ));
                    vis.push(parse_quote!());
                } else if field.is_optional() {
                    tys.push(parse_quote_spanned!(ty.span()=> OptionalConfig<#ty>));
                    vis.push(match field.reader().private {
                        true => parse_quote!(),
                        false => parse_quote!(pub),
                    });
                } else if let Some(ro_field) = field.read_only() {
                    let name = ro_field.lazy_reader_name(&parents);
                    let parent_ty = ro_field.parent_name(&parents);
                    tys.push(parse_quote_spanned!(ro_field.ty.span()=> #name));
                    lazy_readers.push((
                        name,
                        &ro_field.ty,
                        &ro_field.readonly.function,
                        parent_ty,
                        ro_field.ident.clone(),
                        parse_quote!(()),
                        visibility(field),
                    ));
                    vis.push(parse_quote!());
                } else {
                    tys.push(ty.to_owned());
                    vis.push(match field.reader().private {
                        true => parse_quote!(),
                        false => parse_quote!(pub),
                    });
                }
                sub_readers.push(None);
            }
            FieldOrGroup::Multi(group) if !group.reader.skip => {
                let sub_reader_name = prefixed_type_name(name, group);
                idents.push(&group.ident);
                tys.push(parse_quote_spanned!(group.ident.span()=> MultiReader<#sub_reader_name>));
                let mut parents = parents.clone();
                parents.push(PathItem::Static(group.ident.clone(), item.name().into()));
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
                parents.push(PathItem::Static(group.ident.clone(), item.name().into()));
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

    let lazy_reader_impls = lazy_readers.iter().map(
        |(_, ty, function, parent_ty, id, _dto_ty, vis)| -> syn::ItemImpl {
            if let Some((ok, err)) = extract_type_from_result(ty) {
                parse_quote_spanned! {function.span()=>
                    impl #parent_ty {
                        #vis fn #id(&self) -> Result<&#ok, #err> {
                            self.#id.0.get_or_try_init(|| #function(self, &self.#id.1))
                        }
                    }
                }
            } else {
                parse_quote_spanned! {function.span()=>
                    impl #parent_ty {
                        #vis fn #id(&self) -> &#ty {
                            self.#id.0.get_or_init(|| #function(self, &self.#id.1))
                        }
                    }
                }
            }
        },
    );

    let (lr_names, lr_tys, lr_dto_tys): (Vec<_>, Vec<_>, Vec<_>) = lazy_readers
        .iter()
        .map(
            |(name, ty, _, _, _, dto_ty, _)| match extract_type_from_result(ty) {
                Some((ok, _err)) => (name, ok, dto_ty),
                None => (name, *ty, dto_ty),
            },
        )
        .multiunzip();

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
            #[derive(::serde::Serialize, Clone, Debug)]
            #[serde(into = "()")] // Just a hack to support serialization, required for doku
            pub struct #lr_names(::once_cell::sync::OnceCell<#lr_tys>, #lr_dto_tys);

            impl From<#lr_names> for () {
                fn from(_: #lr_names) {}
            }

            #lazy_reader_impls
        )*

        #(#sub_readers)*
    })
}

fn visibility(field: &ConfigurableField) -> syn::Visibility {
    if field.reader().private {
        parse_quote!()
    } else {
        parse_quote!(pub)
    }
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
    Static(syn::Ident, String),
    /// A dynamic field that will be replaced by `.try_get(key0)` when reading the field
    Dynamic(Span),
}

impl PathItem {
    pub fn as_static(&self) -> Option<&syn::Ident> {
        match self {
            Self::Static(s, _rename) => Some(s),
            Self::Dynamic(_) => None,
        }
    }

    pub fn rename(&self) -> Option<&str> {
        match self {
            Self::Static(_, rename) => Some(rename),
            Self::Dynamic(_) => None,
        }
    }
}

fn read_field(parents: &[PathItem]) -> impl Iterator<Item = TokenStream> + '_ {
    let mut id_gen = SequentialIdGenerator::default();
    let mut parent_key = String::new();
    parents.iter().map(move |parent| match parent {
        PathItem::Static(name, _rename) => {
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
        ConfigurableField::ReadWrite(rw_field) => {
            let mut ident: String = parents
                .iter()
                .filter_map(PathItem::rename)
                .map(|name| name.to_pascal_case())
                .collect();
            let mut id_gen = SequentialIdGenerator::default();
            ident.push_str(&field.name().to_pascal_case());
            let ident = syn::Ident::new(&ident, rw_field.ident.span());
            let args = parents.iter().fold(Vec::new(), |mut args, p| {
                if let PathItem::Dynamic(span) = p {
                    args.push(id_gen.next_id(*span));
                }
                args
            });
            let key: syn::Expr = if args.is_empty() {
                parse_quote!(ReadableKey::#ident.to_cow_str())
            } else {
                parse_quote!(ReadableKey::#ident(#(#args.map(<_>::to_owned)),*).to_cow_str())
            };
            let read_path = read_field(parents);
            let value = match &rw_field.default {
                FieldDefault::None => quote_spanned! {rw_field.ident.span()=>
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

                    let (default, value) = if rw_field.reader.function.is_some() {
                        (
                            quote_spanned!(default_key.span()=> #default.1.into()),
                            quote_spanned!(rw_field.ident.span()=> OptionalConfig::Present { value: value.clone(), key: #key }),
                        )
                    } else if matches!(&rw_field.default, FieldDefault::FromOptionalKey(_)) {
                        (
                            quote_spanned!(default_key.span()=> #default.map(|v| v.into())),
                            quote_spanned!(rw_field.ident.span()=> OptionalConfig::Present { value: value.clone(), key: #key }),
                        )
                    } else {
                        (
                            quote_spanned!(default_key.span()=> #default.into()),
                            quote_spanned!(rw_field.ident.span()=> value.clone()),
                        )
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
            };
            if field.reader_function().is_some() {
                let name = rw_field.lazy_reader_name(parents);
                quote_spanned! {rw_field.ident.span()=>
                    #name(<_>::default(), #value)
                }
            } else {
                value
            }
        }
        ConfigurableField::ReadOnly(field) => {
            let name = field.lazy_reader_name(parents);
            quote_spanned! {field.ident.span()=>
                #name(<_>::default(), ())
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
        let root_field = fields.iter().find(|fog| fog.ident() == field).unwrap();
        new_parents.push(PathItem::Static(
            field.to_owned(),
            root_field.name().to_string(),
        ));
        if let Some(PathItem::Static(s, _rename)) = parents.next() {
            if field == s {
                if let Some(PathItem::Dynamic(span)) = parents.peek() {
                    parents.next();
                    new_parents.push(PathItem::Dynamic(*span));
                }
            } else {
                // This key has diverged from the current key's parents, so empty the list
                // This will prevent unwanted matches that aren't genuine
                while parents.next().is_some() {}
            }
        }
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
            PathItem::Static(_, _) => None,
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
                field_conversions.push(quote_spanned!(name.span()=> #name: #value));
            }
            FieldOrGroup::Group(group) if !group.reader.skip => {
                let sub_reader_name = prefixed_type_name(name, group);
                let name = &group.ident;

                let mut parents = parents.clone();
                parents.push(PathItem::Static(
                    group.ident.clone(),
                    item.name().to_string(),
                ));
                let mut id_gen = SequentialIdGenerator::default();
                let extra_call_args: Vec<_> = parents
                    .iter()
                    .filter_map(|path_item| match path_item {
                        PathItem::Static(_, _) => None,
                        PathItem::Dynamic(span) => Some(id_gen.next_id(*span)),
                    })
                    .collect();
                field_conversions.push(
                    quote_spanned!(name.span()=> #name: #sub_reader_name::from_dto(dto, location, #(#extra_call_args),*)),
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
                        PathItem::Static(_, _rename) => None,
                        PathItem::Dynamic(span) => Some(id_gen.next_id(*span)),
                    })
                    .collect();
                let mut parents = parents.clone();
                parents.push(PathItem::Static(
                    group.ident.clone(),
                    item.name().to_string(),
                ));
                let read_path = read_field(&parents);
                #[allow(unstable_name_collisions)]
                let parent_key = parents
                    .iter()
                    .filter_map(|p| match p {
                        PathItem::Static(s, _) => Some(s.to_string()),
                        _ => None,
                    })
                    .intersperse(".".to_owned())
                    .collect::<String>();
                let new_arg2 = extra_call_args.last().unwrap().clone();
                field_conversions.push(quote_spanned!(name.span()=> #name: dto.#(#read_path).*.map_keys(|#new_arg2| #sub_reader_name::from_dto(dto, location, #(#extra_call_args),*), #parent_key)));
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

    Ok(quote_spanned! {name.span()=>
        impl #name {
            #[allow(unused, clippy::clone_on_copy, clippy::useless_conversion)]
            #[automatically_derived]
            /// Converts the provided [TEdgeConfigDto] into a reader
            pub(crate) fn from_dto(dto: &TEdgeConfigDto, location: &TEdgeConfigLocation, #(#extra_args,)*) -> Self {
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
    use syn::Item;
    use syn::ItemImpl;
    use syn::ItemStruct;

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
                PathItem::Static(parse_quote!(c8y), "c8y".into()),
                PathItem::Dynamic(Span::call_site()),
            ],
            &input.groups,
            vec![],
        )
        .unwrap();
        let actual: syn::File = parse_quote!(fn dummy() { #actual });
        let c8y_http_key = quote! {
            ReadableKey::C8yHttp(key0.map(<_>::to_owned)).to_cow_str()
        };
        let c8y_url_key = quote! {
            ReadableKey::C8yUrl(key0.map(<_>::to_owned)).to_cow_str()
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
            &[PathItem::Static(parse_quote!(az), "az".into())],
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
                pub(crate) fn from_dto(
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

    #[test]
    fn generate_structs_generates_getter_for_readonly_value() {
        let input: crate::input::Configuration = parse_quote!(
            device: {
                #[tedge_config(readonly(
                    write_error = "\
                        The device id is read from the device certificate and cannot be set directly.\n\
                        To set 'device.id' to some <id>, you can use `tedge cert create --device-id <id>`.",
                    function = "device_id",
                ))]
                id: String,
            },
        );
        let actual = generate_structs(
            &parse_quote!(TEdgeConfigReader),
            &input.groups,
            Vec::new(),
            "",
        )
        .unwrap();
        let file: syn::File = syn::parse2(actual).unwrap();

        let expected = parse_quote! {
            #[derive(::doku::Document, ::serde::Serialize, Debug, Clone)]
            #[non_exhaustive]
            pub struct TEdgeConfigReader {
                pub device: TEdgeConfigReaderDevice,
            }
            #[derive(::doku::Document, ::serde::Serialize, Debug, Clone)]
            #[non_exhaustive]
            pub struct TEdgeConfigReaderDevice {
                id: LazyReaderDeviceId,
            }
            #[derive(::serde::Serialize, Clone, Debug)]
            #[serde(into = "()")]
            pub struct LazyReaderDeviceId(::once_cell::sync::OnceCell<String>, ());
            impl From<LazyReaderDeviceId> for () {
                fn from(_: LazyReaderDeviceId) {}
            }
            impl TEdgeConfigReaderDevice {
                pub fn id(&self) -> &String {
                    self.id.0.get_or_init(|| device_id(self, &self.id.1))
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&file),
            prettyplease::unparse(&expected)
        )
    }

    #[test]
    fn generate_structs_generates_getter_for_reader_function_value() {
        let input: crate::input::Configuration = parse_quote!(
            device: {
                #[tedge_config(reader(function = "device_id", private))]
                id: String,
            },
        );
        let actual = generate_structs(
            &parse_quote!(TEdgeConfigReader),
            &input.groups,
            Vec::new(),
            "",
        )
        .unwrap();
        let mut file: syn::File = syn::parse2(actual).unwrap();
        let target: syn::Type = parse_quote!(TEdgeConfigReaderDevice);
        file.items
            .retain(|i| matches!(i, Item::Impl(ItemImpl { self_ty, .. }) if **self_ty == target));

        let expected = parse_quote! {
            impl TEdgeConfigReaderDevice {
                fn id(&self) -> &String {
                    self.id.0.get_or_init(|| device_id(self, &self.id.1))
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&file),
            prettyplease::unparse(&expected)
        )
    }

    #[test]
    fn fields_are_public_only_if_directly_readable() {
        let input: crate::input::Configuration = parse_quote!(
            test: {
                #[tedge_config(reader(function = "device_id"))]
                read_via_function: String,
                #[tedge_config(readonly(write_error = "TODO", function="device_id"))]
                readonly: String,
                #[tedge_config(default(value = "test"))]
                with_default: String,
                optional: String,
            },
        );
        let actual = generate_structs(
            &parse_quote!(TEdgeConfigReader),
            &input.groups,
            Vec::new(),
            "",
        )
        .unwrap();
        let mut file: syn::File = syn::parse2(actual).unwrap();
        file.items.retain(|s| matches!(s, Item::Struct(ItemStruct { ident, ..}) if ident == "TEdgeConfigReaderTest"));

        let expected = parse_quote! {
            #[derive(::doku::Document, ::serde::Serialize, Debug, Clone)]
            #[non_exhaustive]
            pub struct TEdgeConfigReaderTest {
                read_via_function: LazyReaderTestReadViaFunction,
                readonly: LazyReaderTestReadonly,
                pub with_default: String,
                pub optional: OptionalConfig<String>,
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&file),
            prettyplease::unparse(&expected)
        )
    }

    #[test]
    fn default_values_do_stuff() {
        let input: crate::input::Configuration = parse_quote!(
            c8y: {
                #[tedge_config(default(from_optional_key = "c8y.url"))]
                http: String,
                url: String,
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

        let expected = parse_quote! {
            impl TEdgeConfigReader {
                #[allow(unused, clippy::clone_on_copy, clippy::useless_conversion)]
                #[automatically_derived]
                /// Converts the provided [TEdgeConfigDto] into a reader
                pub(crate) fn from_dto(dto: &TEdgeConfigDto, location: &TEdgeConfigLocation) -> Self {
                    Self {
                        c8y: TEdgeConfigReaderC8y::from_dto(dto, location),
                    }
                }
            }
            impl TEdgeConfigReaderC8y {
                #[allow(unused, clippy::clone_on_copy, clippy::useless_conversion)]
                #[automatically_derived]
                /// Converts the provided [TEdgeConfigDto] into a reader
                pub(crate) fn from_dto(dto: &TEdgeConfigDto, location: &TEdgeConfigLocation) -> Self {
                    Self {
                        http: match &dto.c8y.http {
                            Some(value) => {
                                OptionalConfig::Present {
                                    value: value.clone(),
                                    key: ReadableKey::C8yHttp.to_cow_str(),
                                }
                            }
                            None => {
                                match &dto.c8y.url {
                                    None => OptionalConfig::Empty(ReadableKey::C8yUrl.to_cow_str()),
                                    Some(value) => {
                                        OptionalConfig::Present {
                                            value: value.clone(),
                                            key: ReadableKey::C8yUrl.to_cow_str(),
                                        }
                                    }
                                }
                                    .map(|v| v.into())
                            }
                        },
                        url: match &dto.c8y.url {
                            None => OptionalConfig::Empty(ReadableKey::C8yUrl.to_cow_str()),
                            Some(value) => {
                                OptionalConfig::Present {
                                    value: value.clone(),
                                    key: ReadableKey::C8yUrl.to_cow_str(),
                                }
                            }
                        },
                    }
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&file),
            prettyplease::unparse(&expected)
        )
    }

    #[test]
    fn default_values_do_stuff2() {
        let input: crate::input::Configuration = parse_quote!(
            device: {
                #[tedge_config(reader(function = "device_id"))]
                id: Result<String, ReadError>,
            },
            c8y: {
                device: {
                    #[tedge_config(default(from_optional_key = "device.id"))]
                    #[tedge_config(reader(function = "c8y_device_id"))]
                    id: Result<String, ReadError>,
                },
            },
        );
        let actual = generate_conversions(
            &parse_quote!(TEdgeConfigReader),
            &input.groups,
            Vec::new(),
            &input.groups,
        )
        .unwrap();
        let mut file: syn::File = syn::parse2(actual).unwrap();
        let target: syn::Type = parse_quote!(TEdgeConfigReaderC8yDevice);
        file.items
            .retain(|i| matches!(i, Item::Impl(ItemImpl { self_ty, ..}) if **self_ty == target));

        let expected = parse_quote! {
            impl TEdgeConfigReaderC8yDevice {
                #[allow(unused, clippy::clone_on_copy, clippy::useless_conversion)]
                #[automatically_derived]
                /// Converts the provided [TEdgeConfigDto] into a reader
                pub(crate) fn from_dto(dto: &TEdgeConfigDto, location: &TEdgeConfigLocation) -> Self {
                    Self {
                        id: LazyReaderC8yDeviceId(
                            <_>::default(),
                            match &dto.c8y.device.id {
                                Some(value) => {
                                    OptionalConfig::Present {
                                        value: value.clone(),
                                        key: ReadableKey::C8yDeviceId.to_cow_str(),
                                    }
                                }
                                None => {
                                    LazyReaderDeviceId(
                                            <_>::default(),
                                            match &dto.device.id {
                                                None => {
                                                    OptionalConfig::Empty(ReadableKey::DeviceId.to_cow_str())
                                                }
                                                Some(value) => {
                                                    OptionalConfig::Present {
                                                        value: value.clone(),
                                                        key: ReadableKey::DeviceId.to_cow_str(),
                                                    }
                                                }
                                            },
                                        )
                                        .1
                                        .into()
                                }
                            },
                        ),
                    }
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&file),
            prettyplease::unparse(&expected)
        )
    }
}
