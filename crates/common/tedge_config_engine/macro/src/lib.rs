use proc_macro::TokenStream;

#[proc_macro]
pub fn define_config(input: TokenStream) -> TokenStream {
    tedge_config_engine_impl::generate_configuration(input.into()).into()
}
