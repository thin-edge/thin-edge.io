use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_quote, parse_quote_spanned};

use crate::{input::FieldOrGroup, prefixed_type_name, utils::extract_type_from_option};

pub fn generate(name: proc_macro2::Ident, items: &[FieldOrGroup]) -> TokenStream {
    let mut idents = Vec::new();
    let mut tys = Vec::<syn::Type>::new();
    let mut sub_dtos = Vec::new();
    let mut attrs = Vec::new();

    for item in items {
        match item {
            FieldOrGroup::Field(field) => {
                if !field.dto().skip && field.read_only().is_none() {
                    idents.push(field.ident());
                    tys.push(make_optional(field.ty()));
                    sub_dtos.push(None);
                    attrs.push(field.attrs().iter().filter(is_preserved).collect());
                }
            }
            FieldOrGroup::Group(group) => {
                if !group.dto.skip {
                    let sub_dto_name = prefixed_type_name(&name, group);
                    idents.push(&group.ident);
                    tys.push(parse_quote_spanned!(group.ident.span()=> #sub_dto_name));
                    sub_dtos.push(Some(generate(sub_dto_name, &group.contents)));
                    attrs.push(Vec::new());
                }
            }
        }
    }

    quote! {
        #[derive(Debug, Default, ::serde::Deserialize, ::serde::Serialize)]
        #[non_exhaustive]
        pub struct #name {
            #(
                #(#attrs)*
                #idents: #tys,
            )*
        }

        #(#sub_dtos)*
    }
}

fn make_optional(ty: &syn::Type) -> syn::Type {
    let non_optional = extract_type_from_option(ty).unwrap_or(ty);
    parse_quote!(Option<#non_optional>)
}

fn is_preserved(attr: &&syn::Attribute) -> bool {
    match attr.parse_meta() {
        // Maybe cfg is useful. Certainly seems sensible to preserve it
        Ok(syn::Meta::List(list)) => list.path.is_ident("serde") || list.path.is_ident("cfg"),
        Ok(syn::Meta::NameValue(nv)) => nv.path.is_ident("doc"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_options_are_preserved_in_dto_field() {
        assert_eq!(
            make_optional(&parse_quote!(Option<String>)),
            parse_quote!(Option<String>)
        );
    }

    #[test]
    fn fields_without_option_are_wrapped_in_dto_field() {
        assert_eq!(
            make_optional(&parse_quote!(String)),
            parse_quote!(Option<String>),
        );
    }

    #[test]
    fn doc_comments_are_preserved() {
        assert!(is_preserved(&&parse_quote!(
            /// Test
        )))
    }

    #[test]
    fn serde_attributes_are_preserved() {
        assert!(is_preserved(&&parse_quote!(
            #[serde(rename = "something")]
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
}
