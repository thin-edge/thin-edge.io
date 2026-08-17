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
    let test_tokens = generate_schema_test(&config, &model);

    let mod_name = quote::format_ident!(
        "__tedge_generated_{}",
        config.name.to_string().to_lowercase()
    );
    let re_exports: Vec<_> = model
        .root
        .all_idents()
        .into_iter()
        .map(|id| quote! { pub use #mod_name::#id; })
        .collect();

    quote! {
        mod #mod_name {
            use super::*;
            use ::tedge_config_engine as tedge;

            #dto_tokens
            #reader_tokens
            #schema_tokens
            #test_tokens
        }
        #(#re_exports)*
    }
}

/// Emits the test that checks the schema's own defaults and examples
///
/// The test lives inside the generated module, so its name cannot collide with
/// another `define_config!` in the same crate.
fn generate_schema_test(config: &Configuration, model: &Model) -> TokenStream {
    if config.skip_generated_test {
        return TokenStream::new();
    }
    let reader_ident = &model.root.reader_ident;
    quote! {
        #[test]
        fn defaults_and_examples_are_valid() {
            if let Err(problems) =
                tedge::check_schema::<#reader_ident>(::std::path::Path::new("/etc/tedge"))
            {
                panic!("{problems}");
            }
        }
    }
}
