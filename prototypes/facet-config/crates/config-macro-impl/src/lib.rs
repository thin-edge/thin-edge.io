use proc_macro2::TokenStream;
use quote::quote;
use syn::parse2;

use input::Configuration;

mod dto;
pub mod input;
mod reader;
mod registries;
#[cfg(test)]
mod test_utils;

pub fn generate_configuration(input: TokenStream) -> TokenStream {
    let config: Configuration = match parse2(input) {
        Ok(c) => c,
        Err(e) => return e.to_compile_error(),
    };

    let root_name = config.name.to_string();

    let dto_tokens = dto::generate_dto(&config, &root_name);
    let reader_tokens = reader::generate_reader(&config, &root_name);
    let registry_tokens = registries::generate_registries(&config);

    quote! {
        #dto_tokens
        #reader_tokens
        #registry_tokens
    }
}
