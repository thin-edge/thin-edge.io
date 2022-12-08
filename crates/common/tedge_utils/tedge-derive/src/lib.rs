use proc_macro::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::{parse, parse_macro_input, ItemStruct};

#[proc_macro_attribute]
/// Adds a field called "other" to a deserialize-able struct.
///
///
/// # Example:
/// ```rust
/// use serde::Deserialize;
///
/// #[tedge_derive::serde_other]
/// #[derive(Deserialize, Default)]
/// struct Foo {};
///
/// let foo = Foo::default();
///
/// assert!(foo.other.is_empty());
/// ```
///
///
pub fn serde_other(args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item_struct = parse_macro_input!(input as ItemStruct);
    let _ = parse_macro_input!(args as parse::Nothing);

    if let syn::Fields::Named(ref mut fields) = item_struct.fields {
        let field_or_err = syn::Field::parse_named.parse2(quote! {
            #[serde(flatten)]
            pub(crate) other: std::collections::BTreeMap<String, toml::Value>
        });

        if let Ok(field) = field_or_err {
            fields.named.push(field);
        }
    }

    quote! {
        #item_struct
    }
    .into()
}
