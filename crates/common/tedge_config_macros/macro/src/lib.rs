//! This crate implements the macro for `tedge_config_macros` and should not be used directly.
extern crate proc_macro;

use proc_macro::TokenStream;
use syn::parse_macro_input;

#[proc_macro]
/// Defines the necessary structures to create a tedge config struct
///
/// # Output
/// This macro outputs a few different types:
/// - `TEdgeConfigDto` --- A data-transfer object, used for reading and writing to toml
/// - `TEdgeConfigReader` --- A struct to read configured values from, populating values with defaults if they exist
pub fn define_tedge_config(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as proc_macro2::TokenStream);

    match tedge_config_macros_impl::generate_configuration(item) {
        Ok(tokens) => tokens.into(),
        Err(err) => TokenStream::from(err.to_compile_error()),
    }
}
