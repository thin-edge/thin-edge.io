use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;
use syn::parse_quote_spanned;
use syn::spanned::Spanned;

use crate::error::extract_type_from_result;
use crate::input::FieldOrGroup;
use crate::prefixed_type_name;

pub fn generate(
    name: proc_macro2::Ident,
    items: &[FieldOrGroup],
    doc_comment: &str,
) -> TokenStream {
    let mut idents = Vec::new();
    let mut tys = Vec::<syn::Type>::new();
    let mut sub_dtos = Vec::new();
    let mut preserved_attrs: Vec<Vec<&syn::Attribute>> = Vec::new();
    let mut extra_attrs = Vec::new();

    for item in items {
        match item {
            FieldOrGroup::Field(field) => {
                if field.reader_function().is_some() {
                    let ty = match extract_type_from_result(field.ty()) {
                        Some((ok, _err)) => ok,
                        None => field.ty(),
                    };
                    idents.push(field.ident());
                    tys.push(parse_quote_spanned!(ty.span() => Option<#ty>));
                    sub_dtos.push(None);
                    preserved_attrs.push(field.attrs().iter().filter(is_preserved).collect());
                    extra_attrs.push(quote! {});
                } else if !field.dto().skip && field.read_only().is_none() {
                    idents.push(field.ident());
                    tys.push({
                        let ty = field.ty();
                        parse_quote_spanned!(ty.span()=> Option<#ty>)
                    });
                    sub_dtos.push(None);
                    preserved_attrs.push(field.attrs().iter().filter(is_preserved).collect());
                    extra_attrs.push(quote! {});
                }
            }
            FieldOrGroup::Group(group) => {
                if !group.dto.skip {
                    let sub_dto_name = prefixed_type_name(&name, group);
                    let is_default = format!("{sub_dto_name}::is_default");
                    idents.push(&group.ident);
                    tys.push(parse_quote_spanned!(group.ident.span()=> #sub_dto_name));
                    sub_dtos.push(Some(generate(sub_dto_name, &group.contents, "")));
                    preserved_attrs.push(group.attrs.iter().filter(is_preserved).collect());
                    extra_attrs.push(quote! {
                        #[serde(default)]
                        #[serde(skip_serializing_if = #is_default)]
                    });
                }
            }
            FieldOrGroup::Multi(group) => {
                if !group.dto.skip {
                    let sub_dto_name = prefixed_type_name(&name, group);
                    idents.push(&group.ident);
                    let field_ty =
                        parse_quote_spanned!(group.ident.span()=> MultiDto<#sub_dto_name>);
                    tys.push(field_ty);
                    sub_dtos.push(Some(generate(sub_dto_name, &group.contents, "")));
                    preserved_attrs.push(group.attrs.iter().filter(is_preserved).collect());
                    extra_attrs.push(quote! {
                        #[serde(default)]
                        #[serde(skip_serializing_if = "MultiDto::is_default")]
                    });
                }
            }
        }
    }

    let doc_comment_attr =
        (!doc_comment.is_empty()).then(|| quote_spanned!(name.span()=> #[doc = #doc_comment]));
    quote_spanned! {name.span()=>
        #[derive(Debug, Default, ::serde::Deserialize, ::serde::Serialize, PartialEq)]
        // We will add more configurations in the future, so this is
        // non_exhaustive (see
        // https://doc.rust-lang.org/reference/attributes/type_system.html)
        #[non_exhaustive]
        #doc_comment_attr
        pub struct #name {
            #(
                // The fields are pub as that allows people to easily modify the
                // dto via a mutable borrow
                #(#preserved_attrs)*
                #extra_attrs
                pub #idents: #tys,
            )*
        }

        impl #name {
            // If #name is a profiled configuration, we don't use this method,
            // but it's a pain to conditionally generate it, so just ignore the
            // warning
            #[allow(unused)]
            fn is_default(&self) -> bool {
                self == &Self::default()
            }
        }

        #(#sub_dtos)*
    }
}

fn is_preserved(attr: &&syn::Attribute) -> bool {
    match &attr.meta {
        // Maybe cfg is useful. Certainly seems sensible to preserve it
        syn::Meta::List(list) => list.path.is_ident("serde") || list.path.is_ident("cfg"),
        syn::Meta::NameValue(nv) => nv.path.is_ident("doc"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use proc_macro2::Span;
    use syn::parse_quote;
    use syn::Ident;
    use syn::Item;
    use syn::ItemStruct;

    use super::*;

    #[test]
    fn doc_comments_are_preserved() {
        assert!(is_preserved(&&parse_quote!(
            /// Test
        )))
    }

    #[test]
    fn serde_attributes_are_preserved() {
        assert!(is_preserved(&&parse_quote!(
            #[serde(alias = "something")]
        )))
    }

    #[test]
    fn unrecognised_attributes_are_not_preserved() {
        assert!(!is_preserved(&&parse_quote!(
            #[unknown_crate(unknown_bool)]
        )))
    }

    #[test]
    fn unrecognised_attributes_of_the_wrong_type_are_not_preserved() {
        assert!(!is_preserved(&&parse_quote!(
            #[unknown_attribute = "some value"]
        )))
    }

    #[test]
    fn dto_is_generated() {
        let input: crate::input::Configuration = parse_quote!(
            c8y: {
                url: String,
            },
            sudo: {
                enable: bool,
            },
        );

        let generated = generate_test_dto(&input);
        let expected = parse_quote! {
            #[derive(Debug, Default, ::serde::Deserialize, ::serde::Serialize, PartialEq)]
            #[non_exhaustive]
            pub struct TEdgeConfigDto {
                #[serde(default)]
                #[serde(skip_serializing_if = "TEdgeConfigDtoC8y::is_default")]
                pub c8y: TEdgeConfigDtoC8y,
                #[serde(default)]
                #[serde(skip_serializing_if = "TEdgeConfigDtoSudo::is_default")]
                pub sudo: TEdgeConfigDtoSudo,
            }

            impl TEdgeConfigDto {
                #[allow(unused)]
                fn is_default(&self) -> bool {
                    self == &Self::default()
                }
            }

            #[derive(Debug, Default, ::serde::Deserialize, ::serde::Serialize, PartialEq)]
            #[non_exhaustive]
            pub struct TEdgeConfigDtoC8y {
                pub url: Option<String>,
            }

            impl TEdgeConfigDtoC8y {
                #[allow(unused)]
                fn is_default(&self) -> bool {
                    self == &Self::default()
                }
            }

            #[derive(Debug, Default, ::serde::Deserialize, ::serde::Serialize, PartialEq)]
            #[non_exhaustive]
            pub struct TEdgeConfigDtoSudo {
                pub enable: Option<bool>,
            }

            impl TEdgeConfigDtoSudo {
                #[allow(unused)]
                fn is_default(&self) -> bool {
                    self == &Self::default()
                }
            }
        };

        assert_eq(&generated, &expected);
    }

    #[test]
    fn ok_type_is_extracted_from_reader_function_if_relevant() {
        let input: crate::input::Configuration = parse_quote!(
            device: {
                #[tedge_config(reader(function = "c8y_device_id"))]
                id: Result<String, ReadError>,
            }
        );

        let mut generated = generate_test_dto(&input);
        generated
            .items
            .retain(only_struct_named("TEdgeConfigDtoDevice"));

        let expected = parse_quote! {
            #[derive(Debug, Default, ::serde::Deserialize, ::serde::Serialize, PartialEq)]
            #[non_exhaustive]
            pub struct TEdgeConfigDtoDevice {
                pub id: Option<String>,
            }
        };

        assert_eq(&generated, &expected);
    }

    #[test]
    fn reader_function_type_is_used_verbatim_in_dto_if_not_result() {
        let input: crate::input::Configuration = parse_quote!(
            device: {
                #[tedge_config(reader(function = "c8y_device_id"))]
                id: String,
            }
        );

        let mut generated = generate_test_dto(&input);
        generated
            .items
            .retain(only_struct_named("TEdgeConfigDtoDevice"));

        let expected = parse_quote! {
            #[derive(Debug, Default, ::serde::Deserialize, ::serde::Serialize, PartialEq)]
            #[non_exhaustive]
            pub struct TEdgeConfigDtoDevice {
                pub id: Option<String>,
            }
        };

        assert_eq(&generated, &expected);
    }

    fn generate_test_dto(input: &crate::input::Configuration) -> syn::File {
        let tokens = super::generate(
            Ident::new("TEdgeConfigDto", Span::call_site()),
            &input.groups,
            "",
        );
        syn::parse2(tokens).unwrap()
    }

    fn assert_eq(actual: &syn::File, expected: &syn::File) {
        pretty_assertions::assert_eq!(
            prettyplease::unparse(actual),
            prettyplease::unparse(expected),
        )
    }

    fn only_struct_named(target: &str) -> impl Fn(&Item) -> bool + '_ {
        move |i| matches!(i, Item::Struct(ItemStruct { ident, .. }) if ident == target)
    }
}
