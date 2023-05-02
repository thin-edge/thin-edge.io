//! Generation for the configuration readers
//!
//! When reading the configuration, we want to see default values if nothing has
//! been configured
use std::iter;

use optional_error::OptionalError;
use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;
use syn::parse_quote_spanned;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::Token;

use crate::input::ConfigurableField;
use crate::input::FieldDefault;
use crate::input::FieldOrGroup;
use crate::prefixed_type_name;

pub fn try_generate(
    root_name: proc_macro2::Ident,
    items: &[FieldOrGroup],
) -> syn::Result<TokenStream> {
    let structs = generate_structs(&root_name, items, false)?;
    let conversions = generate_conversions(&root_name, items, vec![], items)?;
    Ok(quote! {
        #structs
        #conversions
    })
}

fn generate_structs(
    name: &proc_macro2::Ident,
    items: &[FieldOrGroup],
    parent_all_or_nothing: bool,
) -> syn::Result<TokenStream> {
    let mut idents = Vec::new();
    let mut tys = Vec::<syn::Type>::new();
    let mut sub_readers = Vec::new();
    let mut attrs: Vec<Vec<syn::Attribute>> = Vec::new();

    for item in items {
        match item {
            FieldOrGroup::Field(field) => {
                let ty = field.ty();
                attrs.push(field.attrs().to_vec());
                idents.push(field.ident());
                if Some(&FieldDefault::None) == field.read_write().map(|f| &f.default)
                    && !parent_all_or_nothing
                {
                    tys.push(parse_quote_spanned!(ty.span()=> OptionalConfig<#ty>));
                } else {
                    tys.push(ty.to_owned());
                }
                sub_readers.push(None);
            }
            FieldOrGroup::Group(group) if parent_all_or_nothing => {
                return Err(syn::Error::new(
                    group.ident.span(),
                    "Sub-groups inside `#[reader(all_or_nothing)]` are not supported",
                ));
            }
            FieldOrGroup::Group(group) => {
                let sub_reader_name = prefixed_type_name(name, group);
                idents.push(&group.ident);
                if group.reader.all_or_nothing {
                    tys.push(parse_quote_spanned!(group.ident.span()=> OptionalConfigGroup<#sub_reader_name>));
                } else {
                    tys.push(parse_quote_spanned!(group.ident.span()=> #sub_reader_name));
                }
                sub_readers.push(Some(generate_structs(
                    &sub_reader_name,
                    &group.contents,
                    group.reader.all_or_nothing,
                )?));
                attrs.push(group.attrs.to_vec());
            }
        }
    }

    Ok(quote! {
        #[derive(::doku::Document, ::serde::Serialize, Debug)]
        #[non_exhaustive]
        pub struct #name {
            #(
                #(#attrs)*
                pub #idents: #tys,
            )*
        }

        #(#sub_readers)*
    })
}

fn find_field<'a>(
    mut fields: &'a [FieldOrGroup],
    path: &Punctuated<syn::Ident, Token![.]>,
) -> syn::Result<&'a ConfigurableField> {
    let mut current_field = None;
    for (i, segment) in path.iter().enumerate() {
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

        let is_last_segment = i == path.len() - 1;
        match target {
            FieldOrGroup::Group(group) => fields = &group.contents,
            FieldOrGroup::Field(_) if is_last_segment => (),
            _ => {
                let subfields = path
                    .iter()
                    .skip(i + 1)
                    .map(<_>::to_string)
                    .collect::<Vec<_>>()
                    .join(".");
                let segments = path
                    .iter()
                    .take(i + 1)
                    .map(<_>::to_string)
                    .collect::<Vec<_>>()
                    .join(".");
                return Err(syn::Error::new(
                    segment.span(),
                    format!("cannot access `{subfields}` because `{segments}` is a configuration field, not a group"),
                ));
            }
        };
        current_field = Some(target);
    }

    match current_field {
        // TODO test this appears
        None => Err(syn::Error::new(path.span(), "path is empty")),
        Some(FieldOrGroup::Group(_)) => Err(syn::Error::new(
            path.span(),
            // TODO test this too
            "path points to a group of fields, not a single field",
        )),
        Some(FieldOrGroup::Field(f)) => Ok(f),
    }
}

fn reader_value_for_field(
    field: &ConfigurableField,
    parents: &[syn::Ident],
    root_fields: &[FieldOrGroup],
) -> syn::Result<TokenStream> {
    let name = field.ident();
    Ok(if let Some(field) = field.read_write() {
        let key = parents
            .iter()
            .map(|p| p.to_string())
            .chain(iter::once(name.to_string()))
            .collect::<Vec<_>>()
            .join(".");
        match &field.default {
            FieldDefault::None => quote! {
                match &dto.#(#parents).*.#name {
                    None => OptionalConfig::Empty(#key),
                    Some(value) => OptionalConfig::Present { value: value.clone(), key: #key },
                }
            },
            FieldDefault::FromPath(path) => {
                let default = reader_value_for_field(
                    find_field(root_fields, path)?,
                    &path
                        .iter()
                        .take(path.len() - 1)
                        .map(<_>::to_owned)
                        .collect::<Vec<_>>(),
                    root_fields,
                )?;
                quote_spanned! {name.span()=>
                    match &dto.#(#parents).*.#name {
                        Some(value) => value.clone(),
                        None => #default,
                    }
                }
            }
            FieldDefault::Function(function) => quote_spanned! {function.span()=>
                match &dto.#(#parents).*.#name {
                    None => TEdgeConfigDefault::<TEdgeConfigDto, _>::call(#function, dto, location),
                    Some(value) => value.clone(),
                }
            },
            FieldDefault::Value(default) => quote_spanned! {name.span()=>
                match &dto.#(#parents).*.#name {
                    None => #default.into(),
                    Some(value) => value.clone(),
                }
            },
            FieldDefault::Variable(default) => quote_spanned! {name.span()=>
                match &dto.#(#parents).*.#name {
                    None => #default.into(),
                    Some(value) => value.clone(),
                }
            },
        }
    } else {
        // TODO deal with read only stuff
        quote! {
            todo!()
        }
    })
}

/// Generate the conversion methods from DTOs to Readers
fn generate_conversions(
    name: &proc_macro2::Ident,
    items: &[FieldOrGroup],
    parents: Vec<syn::Ident>,
    // TODO this is really confusing passing the same thing in twice
    root_fields: &[FieldOrGroup],
) -> syn::Result<TokenStream> {
    let mut field_conversions = Vec::new();
    let mut rest = Vec::new();

    for item in items {
        match item {
            FieldOrGroup::Field(field) => {
                let name = field.ident();
                let value = reader_value_for_field(field, &parents, root_fields)?;
                field_conversions.push(quote!(#name: #value));
            }
            FieldOrGroup::Group(group) => {
                let sub_reader_name = prefixed_type_name(name, group);
                let name = &group.ident;

                let mut parents = parents.clone();
                parents.push(group.ident.clone());
                field_conversions.push(quote!(#name: #sub_reader_name::from_dto(dto, location)));
                let sub_conversions = if group.reader.all_or_nothing {
                    generate_all_or_nothing_struct(&sub_reader_name, &group.contents, parents)
                } else {
                    generate_conversions(&sub_reader_name, &group.contents, parents, root_fields)
                }?;
                rest.push(sub_conversions);
            }
        }
    }

    Ok(quote! {
        impl #name {
            #[allow(unused, clippy::clone_on_copy, clippy::useless_conversion)]
            #[automatically_derived]
            pub fn from_dto(dto: &TEdgeConfigDto, location: &TEdgeConfigLocation) -> Self {
                Self {
                    #(#field_conversions),*
                }
            }
        }

        #(#rest)*
    })
}

fn generate_all_or_nothing_struct(
    name: &proc_macro2::Ident,
    items: &[FieldOrGroup],
    parents: Vec<syn::Ident>,
) -> syn::Result<TokenStream> {
    let fields = items
        .iter()
        .filter_map(|field| match field {
            FieldOrGroup::Field(f) => Some(f),
            _ => None,
        })
        .collect::<Vec<_>>();

    ensure_defaults_not_set_for(&fields)?;

    let names = fields.iter().map(|f| f.ident()).collect::<Vec<_>>();
    let parentss = std::iter::repeat(&parents)
        .take(names.len())
        .collect::<Vec<_>>();
    let nones = std::iter::repeat(quote!(None)).take(names.len());
    // TODO this should ideally cope with the fields being renamed
    let field = parents
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(".");
    let field_strings = names.iter().map(|name| format!("{field}.{name}"));
    let error = format!("Missing some fields in {field}. ");

    Ok(quote! {
        impl #name {
            #[automatically_derived]
            pub fn from_dto(dto: &TEdgeConfigDto, location: &TEdgeConfigLocation) -> OptionalConfigGroup<Self> {
                match (#(dto.#(#parentss).*.#names),*) {
                    (#(Some(#names)),*) => OptionalConfigGroup::Present(Self {
                        #(#names,)*
                    }),
                    (#(#nones),*) => OptionalConfigGroup::Empty(#field),
                    (#(#names),*) => {
                        let mut error = #error.to_owned();
                        let mut filled_fields = vec![];
                        let mut vacant_fields = vec![];
                        #(
                            match #names {
                                Some(_) => filled_fields.push(#field_strings),
                                None => vacant_fields.push(#field_strings),
                            };
                        )*
                        error.push_str(&format!("Either set {:?}, or unset {:?}", vacant_fields, filled_fields));
                        OptionalConfigGroup::Partial(error)
                    }
                }
            }
        }
    })
}

fn ensure_defaults_not_set_for(fields: &Vec<&ConfigurableField>) -> Result<(), syn::Error> {
    let mut error = OptionalError::default();
    for field in fields {
        match field.read_write().map(|f| &f.default) {
            Some(FieldDefault::None) => (),
            Some(d) => error.combine(syn::Error::new(
                // Safe to unwrap because default is not FieldDefault::None
                d.possible_span().unwrap(),
                "sub-fields of all or nothing groups are not allowed to have defaults",
            )),
            None => error.combine(syn::Error::new(
                field.ident().span(),
                "read-only values are not allowed as all or nothing sub-fields",
            )),
        }
    }
    error.try_throw()
}
