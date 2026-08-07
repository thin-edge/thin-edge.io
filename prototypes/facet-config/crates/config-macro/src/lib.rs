use proc_macro::TokenStream;

#[proc_macro]
pub fn define_config(input: TokenStream) -> TokenStream {
    facet_config_macro_impl::generate_configuration(input.into()).into()
}
