//! Implements `define_config!` by generating three related views of a config:
//! a DTO for stored values, a reader for application code, and runtime schema
//! information for defaults and config operations.

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse2;

use input::Configuration;
use model::Model;

mod dto;
pub mod input;
mod model;
mod reader;
mod schema;
#[cfg(test)]
mod test_utils;

pub fn generate_configuration(input: TokenStream) -> TokenStream {
    let config: Configuration = match parse2(input) {
        Ok(c) => c,
        Err(e) => return e.to_compile_error(),
    };

    let model = Model::new(&config);

    let dto_tokens = dto::generate_dto(&model);
    let reader_tokens = reader::generate_reader(&model);
    let schema_tokens = schema::generate_schema(&model);

    quote! {
        #dto_tokens
        #reader_tokens
        #schema_tokens
    }
}
