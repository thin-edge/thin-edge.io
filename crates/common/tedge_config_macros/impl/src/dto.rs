use proc_macro2::TokenStream;
use quote::quote;
use syn::parse_quote_spanned;
use syn::spanned::Spanned;

use crate::input::FieldOrGroup;
use crate::prefixed_type_name;

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
                    tys.push({
                        let ty = field.ty();
                        parse_quote_spanned!(ty.span()=> Option<#ty>)
                    });
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
    use syn::parse_quote;

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
