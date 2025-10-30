use crate::error::extract_type_from_result;
use crate::input::ConfigurableField;
use crate::input::EnumEntry;
use crate::input::FieldOrGroup;
use crate::namegen::IdGenerator;
use crate::namegen::SequentialIdGenerator;
use crate::namegen::UnderscoreIdGenerator;
use crate::CodegenContext;
use heck::ToSnekCase;
use heck::ToUpperCamelCase;
use itertools::Itertools;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;
use quote::quote_spanned;
use std::borrow::Cow;
use std::collections::VecDeque;
use std::iter::once;
use syn::parse_quote;
use syn::parse_quote_spanned;
use syn::spanned::Spanned;

/// Context for code generation containing all namespaced type and function names
#[derive(Clone)]
struct GenerationContext {
    readable_key_name: syn::Ident,
    readonly_key_name: syn::Ident,
    writable_key_name: syn::Ident,
    dto_key_name: syn::Ident,
    write_error_name: syn::Ident,
    parse_key_error_name: syn::Ident,
    dto_name: syn::Ident,
    reader_name: syn::Ident,
}

impl From<&CodegenContext> for GenerationContext {
    fn from(ctx: &CodegenContext) -> Self {
        GenerationContext {
            readable_key_name: format_ident!("{}ReadableKey", ctx.enum_prefix),
            readonly_key_name: format_ident!("{}ReadOnlyKey", ctx.enum_prefix),
            writable_key_name: format_ident!("{}WritableKey", ctx.enum_prefix),
            dto_key_name: format_ident!("{}DtoKey", ctx.enum_prefix),
            write_error_name: format_ident!("{}WriteError", ctx.enum_prefix),
            parse_key_error_name: format_ident!("{}ParseKeyError", ctx.enum_prefix),
            dto_name: ctx.dto_type_name.clone(),
            reader_name: ctx.reader_type_name.clone(),
        }
    }
}

#[derive(Clone, Copy)]
enum FilterRule {
    ReadOnly,
    ReadWrite,
    None,
}

impl FilterRule {
    fn matches(self, segments: &VecDeque<&FieldOrGroup>) -> bool {
        match self {
            Self::ReadOnly => !is_read_write(segments),
            Self::ReadWrite => is_read_write(segments),
            Self::None => true,
        }
    }
}

pub fn generate_writable_keys(ctx: &CodegenContext, items: &[FieldOrGroup]) -> TokenStream {
    let dto_paths = configuration_paths_from(items, Mode::Dto);
    let mut reader_paths = configuration_paths_from(items, Mode::Reader);
    let gen_ctx = GenerationContext::from(ctx);
    let readable_args = configuration_strings(
        reader_paths.iter(),
        FilterRule::None,
        &gen_ctx.readable_key_name,
    );
    let readonly_args = configuration_strings(
        reader_paths.iter(),
        FilterRule::ReadOnly,
        &gen_ctx.readonly_key_name,
    );
    let writable_args = configuration_strings(
        reader_paths.iter(),
        FilterRule::ReadWrite,
        &gen_ctx.writable_key_name,
    );
    let dto_args = configuration_strings(dto_paths.iter(), FilterRule::None, &gen_ctx.dto_key_name);
    let readable_keys = keys_enum(&gen_ctx.readable_key_name, &readable_args, "read from");
    let readonly_keys = keys_enum(
        &gen_ctx.readonly_key_name,
        &readonly_args,
        "read from, but not written to,",
    );
    let write_error_branches: Vec<syn::Arm> = readonly_args
        .1
        .iter()
        .map(|key| {
            if let Some(error) = &key.write_error {
                let pattern = &key.match_shape;
                parse_quote!(
                    Self::#pattern => #error
                )
            } else if key.sub_field_info.is_some() {
                let pattern = &key.match_read_write;
                parse_quote!(
                    #[allow(unused)]
                    Self::#pattern => sub_key.write_error()
                )
            } else {
                unreachable!()
            }
        })
        .collect();
    let writable_keys = keys_enum(&gen_ctx.writable_key_name, &writable_args, "written to");
    let dto_keys = keys_enum(&gen_ctx.dto_key_name, &dto_args, "written to");
    let fromstr_readable =
        generate_fromstr_readable(&gen_ctx.readable_key_name, &readable_args, &gen_ctx);
    let fromstr_readonly =
        generate_fromstr_readable(&gen_ctx.readonly_key_name, &readonly_args, &gen_ctx);
    let fromstr_writable =
        generate_fromstr_writable(&gen_ctx.writable_key_name, &writable_args, &gen_ctx);
    let fromstr_dto = generate_fromstr_writable(&gen_ctx.dto_key_name, &dto_args, &gen_ctx);
    let read_string = generate_string_readers(&reader_paths, &gen_ctx);
    let write_string = generate_string_writers(
        &reader_paths
            .iter()
            .filter(|path| is_read_write(path))
            .cloned()
            .collect::<Vec<_>>(),
        &gen_ctx,
    );

    let reader_paths_vec = reader_paths
        .iter_mut()
        .map(|vd| &*vd.make_contiguous())
        .collect::<Vec<_>>();
    let readable_keys_iter = key_iterators(
        &ctx.reader_type_name,
        &gen_ctx.readable_key_name,
        &parse_quote_spanned!(ctx.reader_type_name.span()=> readable_keys),
        &reader_paths_vec,
        "",
        &[],
    );
    let readonly_keys_iter = key_iterators(
        &ctx.reader_type_name,
        &gen_ctx.readonly_key_name,
        &parse_quote_spanned!(ctx.reader_type_name.span()=> readonly_keys),
        &reader_paths_vec
            .iter()
            .copied()
            .filter(|r| r.last().unwrap().field().unwrap().read_only().is_some())
            .collect::<Vec<_>>(),
        "",
        &[],
    );
    let writable_keys_iter = key_iterators(
        &ctx.reader_type_name,
        &gen_ctx.writable_key_name,
        &parse_quote_spanned!(ctx.reader_type_name.span()=> writable_keys),
        &reader_paths_vec
            .iter()
            .copied()
            .filter(|r| r.last().unwrap().field().unwrap().read_only().is_none())
            .collect::<Vec<_>>(),
        "",
        &[],
    );

    let (static_alias, deprecated_keys) = deprecated_keys(reader_paths.iter());
    let iter_updated = deprecated_keys.iter().map(|k| &k.iter_field);

    let fallback_branch: Option<syn::Arm> = readonly_args
        .0
        .is_empty()
        .then(|| parse_quote!(_ => unreachable!("ReadOnlyKey is uninhabited")));

    // Extract generation context fields for use in quote! block
    let write_error_name = &gen_ctx.write_error_name;
    let writable_key_name = &gen_ctx.writable_key_name;
    let readonly_key_name = &gen_ctx.readonly_key_name;
    let parse_key_error_name = &gen_ctx.parse_key_error_name;
    let reader_type_name = &ctx.reader_type_name;
    let readable_key_name = &gen_ctx.readable_key_name;
    let utility_functions = if ctx.enum_prefix.is_empty() {
        quote! {
            fn replace_aliases(key: String) -> String {
                use ::once_cell::sync::Lazy;
                use ::std::borrow::Cow;
                use ::std::collections::HashMap;
                use ::doku::*;

                static ALIASES: Lazy<HashMap<Cow<'static, str>, Cow<'static, str>>> = Lazy::new(|| {
                    let ty = #reader_type_name::ty();
                    let TypeKind::Struct { fields, transparent: false } = ty.kind else { panic!("Expected struct but got {:?}", ty.kind) };
                    let Fields::Named { fields } = fields else { panic!("Expected named fields but got {:?}", fields)};
                    let mut aliases = struct_field_aliases(None, &fields);
                    #(
                        if let Some(alias) = aliases.insert(Cow::Borrowed(#static_alias), #readable_key_name::#iter_updated.to_cow_str()) {
                            panic!("Duplicate configuration alias for '{}'. It maps to both '{}' and '{}'. Perhaps you provided an incorrect `deprecated_key` for one of these configurations?", #static_alias, alias, #readable_key_name::#iter_updated.to_cow_str());
                        }
                    )*
                    aliases
                });

                ALIASES
                    .get(&Cow::Borrowed(key.as_str()))
                    .map(|c| c.clone().into_owned())
                    .unwrap_or(key)
            }

            fn warn_about_deprecated_key(deprecated_key: String, updated_key: &'static str) {
                use ::once_cell::sync::Lazy;
                use ::std::sync::Mutex;
                use ::std::collections::HashSet;

                static WARNINGS: Lazy<Mutex<HashSet<String>>> = Lazy::new(<_>::default);

                let warning = format!("The key '{}' is deprecated. Use '{}' instead.", deprecated_key, updated_key);
                if WARNINGS.lock().unwrap().insert(deprecated_key) {
                    ::tracing::warn!("{}", warning);
                }
            }
        }
    } else {
        quote! {}
    };

    let write_error = if ctx.enum_prefix.is_empty() {
        quote! {
            #[derive(::thiserror::Error, Debug)]
            /// An error encountered when writing to a configuration value from a
            /// string
            pub enum #write_error_name {
                #[error("Failed to parse input")]
                ParseValue(#[from] Box<dyn ::std::error::Error + Send + Sync>),
                #[error(transparent)]
                Multi(#[from] MultiError),
                #[error("Setting {target} requires {parent} to be set to {parent_expected}, but it is currently set to {parent_actual}")]
                SuperFieldWrongValue {
                    target: #writable_key_name,
                    parent: #writable_key_name,
                    parent_expected: String,
                    parent_actual: String,
                },
            }
        }
    } else {
        quote! {
            #[derive(::thiserror::Error, Debug)]
            /// An error encountered when writing to a configuration value from a
            /// string
            pub enum #write_error_name {
                #[error("Failed to parse input")]
                ParseValue(#[from] Box<dyn ::std::error::Error + Send + Sync>),
                #[error(transparent)]
                Multi(#[from] MultiError),
            }

            impl From<#write_error_name> for WriteError {
                fn from(inner: #write_error_name) -> WriteError {
                    match inner {
                        #write_error_name::ParseValue(e) => WriteError::ParseValue(e),
                        #write_error_name::Multi(e) => WriteError::Multi(e),
                    }
                }
            }
        }
    };

    quote! {
        #readable_keys
        #readonly_keys
        #writable_keys
        #dto_keys
        #fromstr_readable
        #fromstr_readonly
        #fromstr_writable
        #fromstr_dto
        #read_string
        #write_string
        #readable_keys_iter
        #readonly_keys_iter
        #writable_keys_iter
        #write_error

        impl #readonly_key_name {
            fn write_error(&self) -> &'static str {
                match self {
                    #(#write_error_branches,)*
                    #fallback_branch
                }
            }
        }

        #[derive(Debug, ::thiserror::Error)]
        /// An error encountered when parsing a configuration key from a string
        pub enum #parse_key_error_name {
            #[error("{}", .0.write_error())]
            ReadOnly(#readonly_key_name),
            #[error("Unknown key: '{0}'")]
            Unrecognised(String),
        }

        #utility_functions
    }
}

fn sub_field_enum_variant(
    parent_segments: &VecDeque<&FieldOrGroup>,
    sub_field_variant: &syn::Ident,
    sub_field_type: &syn::Ident,
    key_type_suffix: &syn::Ident,
) -> ConfigurationKey {
    let base_ident = ident_for(parent_segments);
    let combined_ident = format_ident!("{base_ident}{sub_field_variant}");

    let parent_multi_count = parent_segments
        .iter()
        .filter(|fog| matches!(fog, FieldOrGroup::Multi(_)))
        .count();

    let parent_opt_strs =
        std::iter::repeat_n::<syn::Type>(parse_quote!(Option<String>), parent_multi_count);
    let sub_field_key_ty_ident = format_ident!("{sub_field_type}{key_type_suffix}");
    let sub_field_key_ty: syn::Type = parse_quote!(#sub_field_key_ty_ident);

    let mut all_types: Vec<syn::Type> = parent_opt_strs.collect();
    all_types.push(sub_field_key_ty);

    let enum_variant =
        parse_quote_spanned!(combined_ident.span()=> #combined_ident(#(#all_types),*));

    let parent_field_names = SequentialIdGenerator::default()
        .take(parent_multi_count)
        .collect::<Vec<_>>();
    let sub_key_ident = syn::Ident::new("sub_key", sub_field_variant.span());
    let mut all_field_names = parent_field_names.clone();
    all_field_names.push(sub_key_ident);

    let match_read_write =
        parse_quote_spanned!(combined_ident.span()=> #combined_ident(#(#all_field_names),*));

    let all_underscores = UnderscoreIdGenerator.take(parent_multi_count + 1);
    let match_shape =
        parse_quote_spanned!(combined_ident.span()=> #combined_ident(#(#all_underscores),*));

    // Generate formatters for sub-field keys
    // Sub-field keys generate a match arm that handles all profiles at once
    let sub_key_name = syn::Ident::new("sub_key", sub_field_variant.span());
    let sub_field_variant_snake = sub_field_variant.to_string().to_lowercase();

    // Extract parent segments without the field
    let parent_segments_without_field: Vec<&FieldOrGroup> = parent_segments
        .iter()
        .copied()
        .take(parent_segments.len().saturating_sub(1))
        .collect();

    let formatters = if parent_multi_count > 0 {
        // For multi-field parents, generate a single formatter that handles all profiles
        let pattern = parse_quote_spanned!(combined_ident.span()=> #combined_ident(#(#parent_field_names),*, #sub_key_name));

        // TODO this vec![].join thing that's going on feels pretty complicated
        let mut multi_field_idx = 0;
        let base_segments: Vec<TokenStream> = parent_segments_without_field
            .iter()
            .map(|segment| match segment {
                FieldOrGroup::Multi(m) => {
                    let field_name = &parent_field_names[multi_field_idx];
                    let m_name = m.ident.to_string();
                    multi_field_idx += 1;
                    let profile_fmt = format!("{m_name}.profiles.{{}}");
                    quote! {
                        if let Some(profile) = #field_name {
                            format!(#profile_fmt, profile)
                        } else {
                            #m_name.to_string()
                        }
                    }
                }
                FieldOrGroup::Group(g) => {
                    let g_name = g.name().to_string();
                    quote! { #g_name.to_string() }
                }
                FieldOrGroup::Field(f) => {
                    let f_name = f.name().to_string();
                    quote! { #f_name.to_string() }
                }
            })
            .collect();

        let base_expr = quote! {
            {
                vec![#(#base_segments),*].join(".")
            }
        };

        vec![(
            pattern,
            parse_quote!(::std::borrow::Cow::Owned(
                format!("{}.{}.{}", #base_expr, #sub_field_variant_snake, #sub_key_name.to_cow_str())
            )),
        )]
    } else {
        // Non-multi parents: just use parent path directly
        let parent_path_str = parent_segments_without_field
            .iter()
            .map(|fog| fog.name())
            .collect::<Vec<_>>()
            .join(".");

        vec![(
            parse_quote_spanned!(combined_ident.span()=> #combined_ident(#sub_key_name)),
            parse_quote!(::std::borrow::Cow::Owned(
                format!("{}.{}.{}", #parent_path_str, #sub_field_variant_snake, #sub_key_name.to_cow_str())
            )),
        )]
    };

    // Generate regex parser for sub-field keys with profiles
    let regex_parser = if parent_multi_count > 0 {
        // Build a regex pattern for sub-field keys with profiles
        // Must properly handle multi-fields in the parent path, not just assume first segment is multi

        // Build the pattern for the parent path
        let mut pattern_parts = Vec::new();
        for segment in &parent_segments_without_field {
            match segment {
                FieldOrGroup::Multi(m) => {
                    pattern_parts.push(format!("{}(?:[\\._]profiles[\\._]([^\\.]+))?", m.name()));
                }
                FieldOrGroup::Group(g) => {
                    pattern_parts.push(g.name().to_string());
                }
                FieldOrGroup::Field(f) => {
                    pattern_parts.push(f.name().to_string());
                }
            }
        }
        let parent_pattern = pattern_parts.join("[\\._]");

        // Sub-field keys are flat at the parent level, append the sub-field variant
        let pattern = format!(
            "^{}[\\._]{}[\\._](.+)$",
            parent_pattern,
            sub_field_variant.to_string().to_lowercase()
        );

        let pattern_lit = syn::LitStr::new(&pattern, sub_field_variant.span());
        let regex_if: syn::ExprIf = parse_quote! {
            if let Some(captures) = ::regex::Regex::new(#pattern_lit).unwrap().captures(value) {
                // Placeholder - will be filled in by generate_fromstr
            }
        };

        Some(regex_if)
    } else {
        None
    };

    ConfigurationKey {
        enum_variant,
        iter_field: parse_quote!(unreachable!("sub-field keys are not iterable")),
        match_shape,
        match_read_write,
        regex_parser,
        field_names: all_field_names,
        formatters,
        insert_profiles: vec![],
        doc_comment: None,
        sub_field_info: Some(SubFieldInfo {
            type_name: sub_field_type.clone(),
        }),
        write_error: None,
    }
}

fn configuration_strings<'a>(
    variants: impl Iterator<Item = &'a VecDeque<&'a FieldOrGroup>>,
    filter_rule: FilterRule,
    key_type_suffix: &syn::Ident,
) -> (Vec<String>, Vec<ConfigurationKey>) {
    variants
        .flat_map(|segments| {
            let configuration_key = enum_variant(segments);
            let base_string = segments
                .iter()
                .map(|variant| variant.name())
                .collect::<Vec<_>>()
                .join(".");

            let mut results = if filter_rule.matches(segments) {
                vec![(base_string.clone(), configuration_key)]
            } else {
                vec![]
            };

            if let Some(FieldOrGroup::Field(field)) = segments.back() {
                if let Some(sub_fields) = field.sub_field_entries() {
                    // For sub-fields, use the path without the parent field name
                    // e.g., mapper.type -> mapper, or mapper.config.type -> mapper.config
                    let parent_path = segments
                        .iter()
                        .take(segments.len() - 1)
                        .map(|variant| variant.name())
                        .collect::<Vec<_>>()
                        .join(".");

                    for entry in sub_fields.iter() {
                        if let EnumEntry::NameAndFields(variant_name, type_name) = entry {
                            // Sub-field keys are flat at the parent level, not nested under the field
                            let sub_config_str = if parent_path.is_empty() {
                                variant_name.to_string().to_snek_case()
                            } else {
                                format!(
                                    "{}.{}",
                                    parent_path,
                                    variant_name.to_string().to_snek_case()
                                )
                            };
                            let sub_config_key = sub_field_enum_variant(
                                segments,
                                variant_name,
                                type_name,
                                key_type_suffix,
                            );
                            results.push((sub_config_str, sub_config_key));
                        }
                    }
                }
            }

            results
        })
        .unzip()
}

fn deprecated_keys<'a>(
    variants: impl Iterator<Item = &'a VecDeque<&'a FieldOrGroup>>,
) -> (Vec<&'a str>, Vec<ConfigurationKey>) {
    variants
        .flat_map(|segments| {
            segments
                .back()
                .unwrap()
                .field()
                .unwrap()
                .deprecated_keys()
                .map(|key| {
                    let configuration_key = enum_variant(segments);
                    (key, configuration_key)
                })
        })
        .multiunzip()
}

fn generate_fromstr(
    type_name: &syn::Ident,
    (configuration_string, configuration_key): &(Vec<String>, Vec<ConfigurationKey>),
    error_case: syn::Arm,
    gen_ctx: &GenerationContext,
) -> TokenStream {
    // Separate regular keys from sub-field keys
    let (regular_strings, regular_keys): (Vec<_>, Vec<_>) = configuration_string
        .iter()
        .zip(configuration_key.iter())
        .filter(|(_, k)| k.sub_field_info.is_none())
        .unzip();

    let sub_fields = configuration_string
        .iter()
        .zip(configuration_key.iter())
        .filter_map(|(s, k)| Some((s, k, k.sub_field_info.as_ref()?)))
        .collect::<Vec<_>>();

    let simplified_configuration_string = regular_strings
        .iter()
        .map(|s| (s.replace('.', "_"), s))
        .map(|(s, _)| quote_spanned!(Span::call_site()=> #s));

    let iter_variant = regular_keys.iter().map(|k| &k.iter_field);

    let main_parse_err = &gen_ctx.parse_key_error_name;
    let readonly_key_name = &gen_ctx.readonly_key_name;

    // Generate sub-field match cases with prefix matching
    let sub_field_match_cases =
        sub_fields
            .iter()
            .map(|(config_str, config_key, sub_field_info)| {
                let enum_variant = &config_key.enum_variant;

                // Construct sub-field type name from identifier and type name (e.g. C8y + ReadableKey)
                let sub_field_type_name =
                    format_ident!("{}{}", &sub_field_info.type_name, type_name);

                let pattern_str = format!("{}_", config_str.replace('.', "_"));
                let variant_ident = &enum_variant.ident;
                let parent_field_count = config_key.field_names.len() - 1; // All except the last (sub_key)
                let prefix_str = format!("{}.", config_str);
                let sub_field_parse_err = format_ident!("{}{}", sub_field_info.type_name, main_parse_err);
                let unrecognised_sub_key_fmt = format!("{prefix_str}{{sub_key}}");

                // For simple pattern matching (non-profiled), use None for each multi-field
                let match_args = if parent_field_count == 0 {
                    quote!(sub_key)
                } else {
                    // Generate None for each parent field
                    let nones = std::iter::repeat_n(quote!(None), parent_field_count);
                    quote!(#(#nones),*, sub_key)
                };

                let res: syn::Arm = parse_quote_spanned!(enum_variant.span()=>
                    key if key.starts_with(#pattern_str) => {
                        let sub_key_str = value.strip_prefix(#prefix_str).unwrap_or(value);
                        let sub_key: #sub_field_type_name = sub_key_str.parse().map_err(|err| match err {
                            #sub_field_parse_err::ReadOnly(sub_key) => {
                                #main_parse_err::ReadOnly(#readonly_key_name::#variant_ident(#match_args))
                            }
                            #sub_field_parse_err::Unrecognised(sub_key) => {
                                #main_parse_err::Unrecognised(format!(#unrecognised_sub_key_fmt))
                            }
                        })?;
                        return Ok(Self::#variant_ident(#match_args))
                    }
                );
                res
            });

    let regex_patterns =
        configuration_key
            .iter()
            // Exclude sub-field keys - they're handled separately
            .filter(|c| c.sub_field_info.is_none())
            .filter_map(|c| Some((c.regex_parser.clone()?, c)))
            .map(|(mut r, c)| {
                let match_read_write = &c.match_read_write;
                let own_branches = c.field_names.iter().enumerate().map::<syn::Stmt, _>(
                    |(n, id)| {
                        let n = n + 1;
                        parse_quote! {
                            let #id = captures.get(#n).map(|re_match| re_match.as_str().to_owned());
                        }
                    },
                );
                r.then_branch = parse_quote!({
                    #(#own_branches)*
                    return Ok(Self::#match_read_write);
                });
                r
            });

    // Generate regex patterns for sub-field keys with profiles
    let sub_field_regex_patterns: Vec<_> = sub_fields
        .iter()
        .filter_map(|(config_str, c, s)| {
            // Only generate if there's a regex_parser (i.e., has profile fields)
            c.regex_parser.clone().map(|r| (r, *config_str, c, *s))
        })
        .map(|(mut r, config_str, key, sub_field_info)| {
            let variant_ident = &key.enum_variant.ident;
            // Construct sub-field type name from identifier and type name (e.g. C8y + ReadableKey)
            let sub_field_type_name = format_ident!("{}{}", sub_field_info.type_name, type_name);

            let all_field_names = &key.field_names;
            let parent_fields = &all_field_names[..all_field_names.len() - 1];
            // The sub_key is captured after all parent field captures
            // Parent fields are at indices 1, 2, ..., parent_fields.len()
            // Sub_key is at index parent_fields.len() + 1
            let sub_key_capture_idx = parent_fields.len() + 1;
            let sub_field_parse_err =
                format_ident!("{}{}", sub_field_info.type_name, main_parse_err);
            let unrecognised_sub_key_fmt = format!("{config_str}.{{sub_key}}");

            // Generate assignments only for parent fields (not the sub_key)
            let own_branches = parent_fields
                .iter()
                .enumerate()
                .map::<syn::Stmt, _>(|(n, id)| {
                    let n = n + 1;
                    parse_quote! {
                        let #id = captures.get(#n).map(|re_match| re_match.as_str().to_owned());
                    }
                });

            // For sub-keys, we need to parse the remainder
            r.then_branch = parse_quote!({
                #(#own_branches)*
                let sub_key_str = captures.get(#sub_key_capture_idx)
                    .map(|re_match| re_match.as_str())
                    .unwrap_or("");
                let sub_key: #sub_field_type_name = sub_key_str.parse().map_err({
                    #(let #parent_fields = #parent_fields.clone();)*
                    |err| match err {
                        #sub_field_parse_err::ReadOnly(sub_key) => {
                            #main_parse_err::ReadOnly(#readonly_key_name::#variant_ident(#(#parent_fields),*, sub_key))
                        }
                        #sub_field_parse_err::Unrecognised(sub_key) => {
                            #main_parse_err::Unrecognised(format!(#unrecognised_sub_key_fmt))
                        }
                    }
                })?;
                return Ok(Self::#variant_ident(#(#parent_fields),*, sub_key));
            });
            r
        })
        .collect();

    let all_regex_patterns = regex_patterns.chain(sub_field_regex_patterns);
    let parse_key_error = &gen_ctx.parse_key_error_name;

    quote! {
        impl ::std::str::FromStr for #type_name {
            type Err = #parse_key_error;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                // If we get an unreachable pattern, it means we have the same key twice
                #[deny(unreachable_patterns)]
                let res = match replace_aliases(value.to_owned()).replace(".", "_").as_str() {
                    #(
                        #simplified_configuration_string => {
                            if value != #regular_strings {
                                warn_about_deprecated_key(value.to_owned(), #regular_strings);
                            }
                            return Ok(Self::#iter_variant)
                        },
                    )*
                    #(
                        #sub_field_match_cases,
                    )*
                    #error_case
                };
                #(#all_regex_patterns;)*
                res
            }
        }
    }
}

fn generate_fromstr_readable(
    type_name: &syn::Ident,
    fields: &(Vec<String>, Vec<ConfigurationKey>),
    gen_ctx: &GenerationContext,
) -> TokenStream {
    let parse_key_error_name = &gen_ctx.parse_key_error_name;
    generate_fromstr(
        type_name,
        fields,
        parse_quote! { _ => Err(#parse_key_error_name::Unrecognised(value.to_owned())) },
        gen_ctx,
    )
}

// TODO test the error messages actually appear
fn generate_fromstr_writable(
    type_name: &syn::Ident,
    fields: &(Vec<String>, Vec<ConfigurationKey>),
    gen_ctx: &GenerationContext,
) -> TokenStream {
    let GenerationContext {
        readonly_key_name,
        parse_key_error_name,
        ..
    } = gen_ctx;
    generate_fromstr(
        type_name,
        fields,
        parse_quote! {
            _ => if let Ok(key) = <#readonly_key_name as ::std::str::FromStr>::from_str(value) {
                Err(#parse_key_error_name::ReadOnly(key))
            } else {
                Err(#parse_key_error_name::Unrecognised(value.to_owned()))
            },
        },
        gen_ctx,
    )
}

fn key_iterators(
    reader_ty: &syn::Ident,
    type_name: &syn::Ident,
    function_name: &syn::Ident,
    fields: &[&[&FieldOrGroup]],
    prefix: &str,
    args: &[syn::Ident],
) -> TokenStream {
    let mut stmts: Vec<syn::Stmt> = Vec::new();
    let mut exprs: VecDeque<syn::Expr> = VecDeque::new();
    let mut complete_fields: Vec<syn::Expr> = Vec::new();
    let mut global: Vec<TokenStream> = Vec::new();
    let chunks = fields
        .iter()
        .chunk_by(|fog| *fog.first().unwrap() as *const FieldOrGroup);
    for (_, fields) in chunks.into_iter() {
        let fields = fields.collect::<Vec<_>>();
        let field = fields.first().unwrap();
        match field.first() {
            Some(FieldOrGroup::Multi(m)) => {
                let ident = &m.ident;
                let upper_ident = m.ident.to_string().to_upper_camel_case();
                let sub_type_name =
                    syn::Ident::new(&format!("{reader_ty}{upper_ident}"), m.ident.span());
                let keys_ident = syn::Ident::new(&format!("{}_keys", ident), ident.span());
                stmts.push(
                    parse_quote!(let #keys_ident = self.#ident.keys().map(|k| Some(k?.to_string())).collect::<Vec<_>>();),
                );
                let prefix = format!("{prefix}{upper_ident}");
                let remaining_fields = fields.iter().map(|fs| &fs[1..]).collect::<Vec<_>>();
                let arg_clone_stmts = args
                    .iter()
                    .map::<syn::Stmt, _>(|arg| parse_quote!(let #arg = #arg.clone();));
                let cloned_args = args
                    .iter()
                    .map::<syn::Expr, _>(|arg| parse_quote!(#arg.clone()));
                let body: syn::Expr = if args.is_empty() {
                    parse_quote! {
                        |#ident| self.#ident.try_get(#ident.as_deref()).unwrap().#function_name(#ident)
                    }
                } else {
                    parse_quote! {
                        {
                            #(#arg_clone_stmts)*
                            move |#ident| {
                                self.#ident.try_get(#ident.as_deref()).unwrap().#function_name(#(#cloned_args,)* #ident)
                            }
                        }
                    }
                };
                stmts.push(parse_quote! {
                    let #keys_ident = #keys_ident
                    .into_iter()
                    .flat_map(#body);
                });
                exprs.push_back(parse_quote!(#keys_ident));

                let mut args = args.to_owned();
                args.push(m.ident.clone());
                global.push(key_iterators(
                    &sub_type_name,
                    type_name,
                    function_name,
                    &remaining_fields,
                    &prefix,
                    &args,
                ));
            }
            Some(FieldOrGroup::Group(g)) => {
                let upper_ident = g.ident.to_string().to_upper_camel_case();
                let sub_type_name =
                    syn::Ident::new(&format!("{reader_ty}{upper_ident}"), g.ident.span());
                let prefix = format!("{prefix}{upper_ident}");
                let remaining_fields = fields.iter().map(|fs| &fs[1..]).collect::<Vec<_>>();
                global.push(key_iterators(
                    &sub_type_name,
                    type_name,
                    function_name,
                    &remaining_fields,
                    &prefix,
                    args,
                ));
                let ident = &g.ident;
                exprs.push_back(parse_quote! {
                    self.#ident.#function_name(#(#args.clone()),*)
                });
            }
            Some(FieldOrGroup::Field(f)) => {
                let ident = f.ident();
                let field_name = format_ident!(
                    "{}{}",
                    prefix,
                    f.name().to_upper_camel_case(),
                    span = ident.span(),
                );
                let arg_tokens = match args.len() {
                    0 => TokenStream::new(),
                    _ => {
                        quote!((#(#args.clone()),*))
                    }
                };
                complete_fields
                    .push(parse_quote_spanned!(ident.span()=> #type_name::#field_name #arg_tokens));
                if let Some(entries) = f.sub_field_entries() {
                    exprs.push_back(parse_quote_spanned!(ident.span()=> {
                        #(let #args = #args.clone();)*
                        self.#ident.or_none().into_iter().flat_map(move |#ident| #ident.#function_name(#(#args.clone()),*))
                    }));
                    let arms = entries.iter().map::<syn::Arm, _>(|entry|
                        match entry {
                            EnumEntry::NameAndFields(name, _inner) => {
                                let field_name = format_ident!("{field_name}{name}");
                                let sub_field_name = name.to_string().to_snek_case();
                                let sub_field_name = format_ident!("{}", sub_field_name, span = name.span());
                                parse_quote!(Self::#name { #sub_field_name } => #sub_field_name.
                                    #function_name()
                                    .map(|inner_key| #type_name::#field_name(#(#args.clone(),)* inner_key))
                                    .collect(),
                                )
                            }
                            EnumEntry::NameOnly(name) => {
                                parse_quote!(Self::#name => Vec::new(),)
                            }
                        }
                    );
                    let impl_for = f.reader_ty();
                    global.push(quote! {
                        impl #impl_for {
                            pub fn #function_name(&self #(, #args: Option<String>)*) -> Vec<#type_name> {
                                match self {
                                    #(#arms)*
                                }
                            }
                        }
                    })
                }
            }
            None => panic!("Expected FieldOrGroup list te be nonempty"),
        };
    }

    if !complete_fields.is_empty() {
        // Iterate through fields before groups
        exprs.push_front(parse_quote!([#(#complete_fields),*].into_iter()));
    }

    if exprs.is_empty() {
        // If the enum is empty, we need something to iterate over, so generate an empty iterator
        exprs.push_back(parse_quote!(std::iter::empty()));
    }
    let exprs = exprs.into_iter().enumerate().map(|(i, expr)| {
        if i > 0 {
            parse_quote_spanned!(function_name.span()=> chain(#expr))
        } else {
            expr
        }
    });

    quote_spanned! {function_name.span()=>
        impl #reader_ty {
            pub fn #function_name(&self #(, #args: Option<String>)*) -> impl Iterator<Item = #type_name> + '_ {
                #(#stmts)*
                #(#exprs).*
            }
        }

        #(#global)*
    }
}

fn keys_enum(
    type_name: &syn::Ident,
    (configuration_string, configuration_key): &(Vec<String>, Vec<ConfigurationKey>),
    doc_fragment: &'static str,
) -> TokenStream {
    let as_str_example = configuration_key
        .iter()
        .map(|k| &k.iter_field)
        .zip(configuration_string.iter())
        .map(|(ident, value)| {
            format!(
                "assert_eq!({type_name}::{ident}.as_str(), \"{value}\");\n",
                ident = quote!(#ident)
            )
        })
        .take(10)
        .collect::<Vec<_>>();
    let as_str_example = (!as_str_example.is_empty()).then(|| {
        quote! {
            /// ```compile_fail
            /// // This doctest is compile_fail because we have no way to import the
            /// // current type, but the example is still valuable
            #(
                #[doc = #as_str_example]
            )*
            /// ```
        }
    });
    let type_name_str = type_name.to_string();
    let enum_variant = configuration_key.iter().map(|k| &k.enum_variant);
    let (fmt_match, fmt_ret): (Vec<_>, Vec<_>) = configuration_key
        .iter()
        .flat_map(|k| k.formatters.clone())
        .unzip();

    let iter_field: Vec<_> = configuration_key
        .iter()
        // Exclude sub-field keys: they aren't (statically) iterable
        // TODO make the keys iterable
        .filter_map(|k| {
            if k.sub_field_info.is_none() {
                Some(k.iter_field.clone())
            } else {
                None
            }
        })
        .collect();
    let uninhabited_catch_all = configuration_key
        .is_empty()
        .then_some::<syn::Arm>(parse_quote!(_ => unimplemented!("Cope with empty enum")));

    let (wp_match, wp_ret): (Vec<_>, Vec<_>) = configuration_key
        .iter()
        .flat_map(|k| k.insert_profiles.clone())
        .unzip();
    let match_shape = configuration_key
        .iter()
        .map(|k| &k.match_shape)
        .collect::<Vec<_>>();
    let doc_comment = configuration_key
        .iter()
        .map(|k| k.doc_comment.as_ref())
        .collect::<Vec<_>>();
    let doc_comment = doc_comment.into_iter().map(|c| match c {
        Some(c) => quote!(Some(#c)),
        None => quote!(None),
    });

    let max_profile_count = configuration_key
        .iter()
        .filter(|k| !k.insert_profiles.is_empty())
        .map(|k| k.field_names.len())
        .max();

    let try_with_profile_impl = match max_profile_count {
        Some(1) => quote! {
            pub fn try_with_profile(self, profile: ProfileName) -> ::anyhow::Result<Self> {
                match self {
                    #(
                        #wp_match => #wp_ret,
                    )*
                    other => {
                        ::anyhow::bail!("You've supplied a profile, but {other} is not a profiled configuration")
                    },
                }
            }
        },

        // If no profiles, just don't implement the method
        Some(0) | None => quote! {},

        // If profiles are nested, we need to rethink this method entirely
        // This likely won't ever be needed, but it's good to have a clear error message if someone does stumble across it
        Some(2..) => {
            let error_loc = format!("{}:{}", file!(), line!() + 1);
            let error = format!("try_with_profile cannot be implemented (in its current form) for nested profiles. You'll need to modify the code at {error_loc} to support this.");
            quote! {
                pub fn try_with_profile(self, profile: ProfileName) -> ::anyhow::Result<Self> {
                    compile_error!(#error)
                }
            }
        }
    };

    quote! {
        #[derive(Clone, Debug, PartialEq, Eq)]
        #[non_exhaustive]
        #[allow(unused)]
        #[doc = concat!("A key that can be *", #doc_fragment, "* the configuration\n\n")]
        #[doc = concat!("This can be converted to `&'static str` using [`", #type_name_str, "::as_str`], and")]
        #[doc = "parsed using [`FromStr`](::std::str::FromStr). The `FromStr` implementation also"]
        #[doc = "automatically emits warnings about deprecated keys. It also implements [Display](std::fmt::Display),"]
        #[doc = "so you can also use it in format strings."]
        pub enum #type_name {
            #(
                #[doc = concat!("`", #configuration_string, "`")]
                #enum_variant,
            )*
        }

        impl #type_name {
            /// Converts this key to the canonical key used by `tedge config` and `tedge.toml`
            #as_str_example
            pub fn to_cow_str(&self) -> ::std::borrow::Cow<'static, str> {
                match self {
                    #(
                        Self::#fmt_match => #fmt_ret,
                    )*
                    #uninhabited_catch_all
                }
            }

            #try_with_profile_impl

            // TODO: Replace VALUES with a mechanism that supports all keys, including sub-fields
            // Currently sub-fields are excluded because the available sub-field keys are generated
            // by a separate macro invocation, so we can't know them statically here
            const VALUES: &'static [Self] = &[
                #(Self::#iter_field),*
            ];
            fn help(&self) -> Option<&'static str> {
                match self {
                    #(
                        Self::#match_shape => #doc_comment,
                    )*
                    #uninhabited_catch_all
                }
            }

            pub fn completions() -> Vec<::clap_complete::CompletionCandidate> {
                Self::VALUES.into_iter().map(|v| ::clap_complete::CompletionCandidate::new(v.to_cow_str().into_owned()).help(v.help().map(|h| h.replace("\n", " ").into()))).collect()
            }
        }

        impl ::std::fmt::Display for #type_name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                self.to_cow_str().fmt(f)
            }
        }
    }
}

fn generate_field_accessor<'a>(
    fields: &'a VecDeque<&FieldOrGroup>,
    method: &'a str,
    // TODO use an enum or something
    exclude_parents: bool,
) -> impl Iterator<Item = TokenStream> + 'a {
    let mut id_gen = SequentialIdGenerator::default();
    let method = syn::Ident::new(method, Span::call_site());
    let mut fields_so_far = Vec::new();
    fields.iter().map(move |field| {
        let ident = field.ident();
        fields_so_far.push(ident);
        match field {
            FieldOrGroup::Field(_) => quote!(#ident),
            FieldOrGroup::Group(_) => quote!(#ident),
            FieldOrGroup::Multi(_) if exclude_parents => {
                // Interacting with TEdgeConfigReader - parents already included in value
                let field = id_gen.next_id(ident.span());
                quote_spanned!(ident.span()=> #ident.#method(#field.as_deref())?)
            }
            FieldOrGroup::Multi(_) => {
                // Interacting with TEdgeConfigDto - parents need to be supplied with try_get_mut
                let field = id_gen.next_id(ident.span());
                #[allow(unstable_name_collisions)]
                let parents = fields_so_far
                    .iter()
                    .map(|id| id.to_string())
                    .intersperse(".".to_owned())
                    .collect::<String>();
                quote_spanned!(ident.span()=> #ident.#method(#field.as_deref(), #parents)?)
            }
        }
    })
}

fn generate_multi_dto_cleanup(fields: &VecDeque<&FieldOrGroup>) -> Vec<syn::Stmt> {
    let mut id_gen = SequentialIdGenerator::default();
    let mut all_idents = Vec::new();
    let mut fields_so_far = Vec::new();
    let mut result = Vec::new();
    for field in fields {
        let ident = field.ident();
        all_idents.push(ident);
        match field {
            FieldOrGroup::Multi(_) => {
                let field = id_gen.next_id(ident.span());
                #[allow(unstable_name_collisions)]
                let parents = all_idents
                    .iter()
                    .map(|id| id.to_string())
                    .intersperse(".".to_owned())
                    .collect::<String>();
                result.push(fields_so_far.iter().cloned().chain(once(quote_spanned!(ident.span()=> #ident.remove_if_empty(#field.as_deref())))).collect::<Vec<_>>());
                fields_so_far.push(
                    quote_spanned!(ident.span()=> #ident.try_get_mut(#field.as_deref(), #parents)?),
                );
            }
            _ => fields_so_far.push(quote!(#ident)),
        }
    }
    result
        .into_iter()
        .rev()
        .map(|fields| parse_quote!(self.#(#fields).*;))
        .collect()
}

fn generate_read_arm_for_field(
    path: &VecDeque<&FieldOrGroup>,
    configuration_key: ConfigurationKey,
    gen_ctx: &GenerationContext,
) -> syn::Arm {
    let field = path
        .back()
        .expect("Path must have a back as it is nonempty")
        .field()
        .expect("Back of path is guaranteed to be a field");
    let segments = generate_field_accessor(path, "try_get", true);
    let to_string = quote_spanned!(field.reader_ty().span()=> .to_string());
    let match_variant = configuration_key.match_read_write;
    let readable_key_name = &gen_ctx.readable_key_name;

    if field.read_only().is_some() || field.reader_function().is_some() {
        if extract_type_from_result(field.reader_ty()).is_some() {
            parse_quote! {
                #readable_key_name::#match_variant => Ok(self.#(#segments).*()?#to_string),
            }
        } else {
            parse_quote! {
                #readable_key_name::#match_variant => Ok(self.#(#segments).*()#to_string),
            }
        }
    } else if field.has_guaranteed_default() {
        parse_quote! {
            #readable_key_name::#match_variant => Ok(self.#(#segments).*#to_string),
        }
    } else {
        parse_quote! {
            #readable_key_name::#match_variant => Ok(self.#(#segments).*.or_config_not_set()?#to_string),
        }
    }
}

fn generate_read_arms_for_sub_fields(path: &VecDeque<&FieldOrGroup>) -> Vec<syn::Arm> {
    let Some(field) = path.back().and_then(|f| f.field()) else {
        return vec![];
    };

    let Some(sub_fields) = field.sub_field_entries() else {
        return vec![];
    };

    let parent_segments = generate_field_accessor(path, "try_get", true).collect::<Vec<_>>();
    let field_ty = field.reader_ty();
    let base_ident = ident_for(path);
    let parent_multi_count = path
        .iter()
        .filter(|fog| matches!(fog, FieldOrGroup::Multi(_)))
        .count();

    sub_fields
        .iter()
        .filter_map(|entry| {
            let EnumEntry::NameAndFields(variant_name, _type_name) = entry else {
                return None;
            };

            let combined_ident = format_ident!("{base_ident}{variant_name}");
            let variant_field_name = syn::Ident::new(
                &variant_name.to_string().to_snek_case(),
                variant_name.span(),
            );
            let parent_field_names = SequentialIdGenerator::default()
                .take(parent_multi_count)
                .collect::<Vec<_>>();
            let sub_key_ident = syn::Ident::new("sub_key", variant_name.span());
            let error_msg = format!(
                "Attempted to read {} sub-field from non-{} variant",
                variant_name, variant_name
            );

            let arm: syn::Arm = parse_quote! {
                ReadableKey::#combined_ident(#(#parent_field_names,)* #sub_key_ident) => {
                    let mapper_ty = &self.#(#parent_segments).*.or_config_not_set()?;
                    match mapper_ty {
                        #field_ty::#variant_name { #variant_field_name } => {
                            #variant_field_name.read_string(#sub_key_ident)
                        }
                        _ => unreachable!(#error_msg),
                    }
                }
            };
            Some(arm)
        })
        .collect()
}

fn generate_string_readers(
    paths: &[VecDeque<&FieldOrGroup>],
    gen_ctx: &GenerationContext,
) -> TokenStream {
    let enum_variants = paths.iter().map(enum_variant);
    let arms = paths
        .iter()
        .zip(enum_variants)
        .flat_map(|(path, configuration_key)| {
            let main_arm = generate_read_arm_for_field(path, configuration_key, gen_ctx);
            let sub_field_arms = generate_read_arms_for_sub_fields(path);
            std::iter::once(main_arm).chain(sub_field_arms)
        });

    let fallback_branch: Option<syn::Arm> = paths
        .is_empty()
        .then(|| parse_quote!(_ => unreachable!("ReadableKey is uninhabited")));
    let reader_name = &gen_ctx.reader_name;
    let readable_key_name = &gen_ctx.readable_key_name;

    quote! {
        impl #reader_name {
            pub fn read_string(&self, key: &#readable_key_name) -> Result<String, ReadError> {
                match key {
                    #(#arms)*
                    #fallback_branch
                }
            }
        }
    }
}

fn generate_write_arms_for_sub_fields(
    path: &VecDeque<&FieldOrGroup>,
) -> Vec<(syn::Arm, syn::Arm, syn::Arm, syn::Arm, syn::Arm)> {
    let Some(field) = path.back().and_then(|f| f.field()) else {
        return vec![];
    };

    let Some(sub_fields) = field.sub_field_entries() else {
        return vec![];
    };

    let mut parent_segments =
        generate_field_accessor(path, "try_get_mut", false).collect::<Vec<_>>();
    // Remove the last segment (the field itself) to get just the parent
    parent_segments.pop();
    let reader_segments = generate_field_accessor(path, "try_get", true).collect::<Vec<_>>();

    let field_dto_ty = field.dto_ty();
    let field_reader_ty = field.reader_ty();
    let field_name = field.ident();
    let base_ident = ident_for(path);
    let parent_multi_count = path
        .iter()
        .filter(|fog| matches!(fog, FieldOrGroup::Multi(_)))
        .count();

    sub_fields
        .iter()
        .filter_map(|entry| {
            let EnumEntry::NameAndFields(variant_name, type_name) = entry else {
                return None;
            };

            let combined_ident = format_ident!("{base_ident}{variant_name}");
            let variant_field_name = syn::Ident::new(&variant_name.to_string().to_snek_case(), variant_name.span());
            let variant_reader_field_name = format_ident!("{variant_field_name}_reader");
            let parent_field_names = SequentialIdGenerator::default().take(parent_multi_count).collect::<Vec<_>>();
            let sub_key_ident = syn::Ident::new("sub_key", variant_name.span());
            let variant_name_str = variant_name.to_string().to_snek_case();
            let dto_type_ident = format_ident!("{}Dto", type_name);
            let reader_type_ident = format_ident!("{}Reader", type_name);

            // Get the parent variable name from the path
            let parent_var_name = if parent_segments.is_empty() {
                // If there are no parent segments, we're working on self directly
                syn::Ident::new("self_root", field_name.span())
            } else {
                // Take the ident from the last element in the path (before the field itself)
                path.iter()
                    .rev()
                    .nth(1)
                    .expect("Parent must exist since parent_segments is not empty")
                    .ident()
                    .clone()
            };

            // Generate the field variable name as {parent}_{field}
            let field_var_name = format_ident!("{}_{}", parent_var_name, field_name);

            let update_arm: syn::Arm = parse_quote_spanned! {entry.span()=>
                WritableKey::#combined_ident(#(#parent_field_names,)* #sub_key_ident) => {
                    let #parent_var_name = self.#(#parent_segments).*;
                    let #field_var_name = #parent_var_name.#field_name.get_or_insert_with(|| #field_dto_ty::#variant_name { #variant_field_name: #dto_type_ident::default() });
                    if let #field_dto_ty::#variant_name { #variant_field_name } = #field_var_name {
                        #variant_field_name.try_update_str(#sub_key_ident, value)?;
                    } else {
                        return Err(WriteError::SuperFieldWrongValue {
                            target: key.clone(),
                            parent: WritableKey::#base_ident(#(#parent_field_names.clone()),*),
                            parent_expected: #variant_name_str.to_string(),
                            parent_actual: #field_var_name.to_string(),
                        });
                    }
                }
            };

            let other_parent_var_name = format_ident!("other_{}", parent_var_name);
            let other_field_var_name = format_ident!("other_{}", field_var_name);
            let other_variant_field_name = format_ident!("other_{}", variant_field_name);
            let reader_var_name = format_ident!("{}_reader", field_var_name);

            let take_value_arm: syn::Arm = parse_quote! {
                WritableKey::#combined_ident(#(#parent_field_names,)* #sub_key_ident) => {
                    let #parent_var_name = self.#(#parent_segments).*;
                    let #field_var_name = #parent_var_name.#field_name.get_or_insert_with(|| #field_dto_ty::#variant_name { #variant_field_name: #dto_type_ident::default() });
                    if let #field_dto_ty::#variant_name { #variant_field_name } = #field_var_name {
                        let #other_parent_var_name = other.#(#parent_segments).*;
                        let #other_field_var_name = &mut #other_parent_var_name.#field_name;
                        if let Some(#field_dto_ty::#variant_name { #variant_field_name: ref mut #other_variant_field_name }) = #other_field_var_name {
                            #variant_field_name.take_value_from(#other_variant_field_name, #sub_key_ident)?;
                        }
                    } else {
                        return Err(WriteError::SuperFieldWrongValue {
                            target: key.clone(),
                            parent: WritableKey::#base_ident(#(#parent_field_names.clone()),*),
                            parent_expected: #variant_name_str.to_string(),
                            parent_actual: #field_var_name.to_string(),
                        });
                    }
                }
            };

            let unset_arm: syn::Arm = parse_quote! {
                WritableKey::#combined_ident(#(#parent_field_names,)* #sub_key_ident) => {
                    let #parent_var_name = self.#(#parent_segments).*;
                    let #field_var_name = #parent_var_name.#field_name.get_or_insert_with(|| #field_dto_ty::#variant_name { #variant_field_name: #dto_type_ident::default() });
                    if let #field_dto_ty::#variant_name { #variant_field_name } = #field_var_name {
                        #variant_field_name.try_unset_key(#sub_key_ident)?;
                    } else {
                        return Err(WriteError::SuperFieldWrongValue {
                            target: key.clone(),
                            parent: WritableKey::#base_ident(#(#parent_field_names.clone()),*),
                            parent_expected: #variant_name_str.to_string(),
                            parent_actual: #field_var_name.to_string(),
                        });
                    }
                }
            };

            let append_arm: syn::Arm = parse_quote! {
                WritableKey::#combined_ident(#(#parent_field_names,)* #sub_key_ident) => {
                    let #parent_var_name = self.#(#parent_segments).*;
                    let #field_var_name = #parent_var_name.#field_name.get_or_insert_with(|| #field_dto_ty::#variant_name { #variant_field_name: #dto_type_ident::default() });
                    let #reader_var_name = reader.#(#reader_segments).*.or_none().map(::std::borrow::Cow::Borrowed).unwrap_or_else(|| {
                        ::std::borrow::Cow::Owned(#field_reader_ty::#variant_name {
                            #variant_field_name: #reader_type_ident::from_dto(
                                &#dto_type_ident::default(),
                                &TEdgeConfigLocation::default(),
                            )
                        })
                    });
                    if let #field_dto_ty::#variant_name { #variant_field_name } = #field_var_name {
                        if let #field_reader_ty::#variant_name { #variant_field_name: #variant_reader_field_name } = #reader_var_name.as_ref() {
                            #variant_field_name.try_append_str(#variant_reader_field_name, #sub_key_ident, value)?;
                        } else {
                            unreachable!("Shape of reader should match shape of DTO")
                        }
                    } else {
                        return Err(WriteError::SuperFieldWrongValue {
                            target: key.clone(),
                            parent: WritableKey::#base_ident(#(#parent_field_names.clone()),*),
                            parent_expected: #variant_name_str.to_string(),
                            parent_actual: #field_var_name.to_string(),
                        });
                    }
                }
            };

            let remove_arm: syn::Arm = parse_quote! {
                WritableKey::#combined_ident(#(#parent_field_names,)* #sub_key_ident) => {
                    let #parent_var_name = self.#(#parent_segments).*;
                    let #field_var_name = #parent_var_name.#field_name.get_or_insert_with(|| #field_dto_ty::#variant_name { #variant_field_name: #dto_type_ident::default() });
                    let #reader_var_name = reader.#(#reader_segments).*.or_none().map(::std::borrow::Cow::Borrowed).unwrap_or_else(|| {
                        ::std::borrow::Cow::Owned(#field_reader_ty::#variant_name {
                            #variant_field_name: #reader_type_ident::from_dto(
                                &#dto_type_ident::default(),
                                &TEdgeConfigLocation::default(),
                            )
                        })
                    });
                    if let #field_dto_ty::#variant_name { #variant_field_name } = #field_var_name {
                        if let #field_reader_ty::#variant_name { #variant_field_name: #variant_reader_field_name } = #reader_var_name.as_ref() {
                            #variant_field_name.try_remove_str(#variant_reader_field_name, #sub_key_ident, value)?;
                        } else {
                            unreachable!("Shape of reader should match shape of DTO")
                        }
                    } else {
                        return Err(WriteError::SuperFieldWrongValue {
                            target: key.clone(),
                            parent: WritableKey::#base_ident(#(#parent_field_names.clone()),*),
                            parent_expected: #variant_name_str.to_string(),
                            parent_actual: #field_var_name.to_string(),
                        });
                    }
                }
            };

            Some((update_arm, take_value_arm, unset_arm, append_arm, remove_arm))
        })
        .collect()
}

fn generate_string_writers(
    paths: &[VecDeque<&FieldOrGroup>],
    gen_ctx: &GenerationContext,
) -> TokenStream {
    let writable_key_name = &gen_ctx.writable_key_name;
    let dto_name = &gen_ctx.dto_name;
    let reader_name = &gen_ctx.reader_name;
    let write_error_name = &gen_ctx.write_error_name;
    let enum_variants = paths.iter().map(enum_variant);
    type Arms = (
        Vec<syn::Arm>,
        Vec<syn::Arm>,
        Vec<syn::Arm>,
        Vec<syn::Arm>,
        Vec<syn::Arm>,
    );
    let (update_arms, take_value_arms, unset_arms, append_arms, remove_arms): Arms  = paths
        .iter()
        .zip(enum_variants)
        .flat_map(|(path, configuration_key)| {
            let read_segments = generate_field_accessor(path, "try_get", true);
            let write_segments = generate_field_accessor(path, "try_get_mut", false).collect::<Vec<_>>();
            let cleanup_stmts = generate_multi_dto_cleanup(path);
            let field = path
                .iter()
                .filter_map(|thing| thing.field())
                .next()
                .unwrap();
            let match_variant = configuration_key.match_read_write;

            let ty = if field.reader_function().is_some() {
                extract_type_from_result(field.dto_ty()).map(|tys| tys.0).unwrap_or(field.dto_ty())
            } else {
                field.dto_ty()
            };

            let parse_as = field.from().unwrap_or(field.dto_ty());
            let parse = quote_spanned! {parse_as.span()=> parse::<#parse_as>() };
            let convert_to_field_ty = quote_spanned! {ty.span()=> map(<#ty>::from)};

            // For fields with sub-fields, get current value from self (Dto) instead of reader,
            // since the reader type is different from the dto type for sub-fields
            let current_value = if field.sub_field_entries().is_some() {
                quote_spanned! {ty.span()=> self.#(#write_segments).*.take()}
            } else if field.read_only().is_some() || field.reader_function().is_some() {
                if extract_type_from_result(field.reader_ty()).is_some() {
                    quote_spanned! {ty.span()=> reader.#(#read_segments).*().ok().cloned()}
                } else {
                    quote_spanned! {ty.span()=> Some(reader.#(#read_segments).*())}
                }
            } else if field.has_guaranteed_default() {
                quote_spanned! {ty.span()=> Some(reader.#(#read_segments).*.to_owned())}
            } else {
                quote_spanned! {ty.span()=> reader.#(#read_segments).*.or_none().cloned()}
            };

            let main_arms = (
                parse_quote_spanned! {ty.span()=>
                    #[allow(clippy::useless_conversion)]
                    #writable_key_name::#match_variant => self.#(#write_segments).* = Some(value
                        .#parse
                        .#convert_to_field_ty
                        .map_err(|e| #write_error_name::ParseValue(Box::new(e)))?),
                },
                parse_quote_spanned! {ty.span()=>
                    #writable_key_name::#match_variant => self.#(#write_segments).* = other.#(#write_segments).*.take(),
                },
                parse_quote_spanned! {ty.span()=>
                    #writable_key_name::#match_variant => {
                        self.#(#write_segments).* = None;
                        #(#cleanup_stmts)*
                    },
                },
                parse_quote_spanned! {ty.span()=>
                    #[allow(clippy::useless_conversion)]
                    #writable_key_name::#match_variant => self.#(#write_segments).* = <#ty as AppendRemoveItem>::append(
                        #current_value,
                        value
                        .#parse
                        .#convert_to_field_ty
                        .map_err(|e| #write_error_name::ParseValue(Box::new(e)))?),
                },
                parse_quote_spanned! {ty.span()=>
                    #[allow(clippy::useless_conversion)]
                    #writable_key_name::#match_variant => self.#(#write_segments).* = <#ty as AppendRemoveItem>::remove(
                        #current_value,
                        value
                        .#parse
                        .#convert_to_field_ty
                        .map_err(|e| #write_error_name::ParseValue(Box::new(e)))?),
                },
            );

            let sub_field_arms = generate_write_arms_for_sub_fields(path);
            std::iter::once(main_arms).chain(sub_field_arms)
        })
        .multiunzip();
    let fallback_branch: Option<syn::Arm> = update_arms
        .is_empty()
        .then(|| parse_quote!(_ => unreachable!("WritableKey is uninhabited")));

    quote! {
        impl #dto_name {
            pub fn try_update_str(&mut self, key: &#writable_key_name, value: &str) -> Result<(), #write_error_name> {
                match key {
                    #(#update_arms)*
                    #fallback_branch
                };
                Ok(())
            }

            pub(crate) fn take_value_from(&mut self, other: &mut #dto_name, key: &#writable_key_name) -> Result<(), #write_error_name> {
                match key {
                    #(#take_value_arms)*
                    #fallback_branch
                };
                Ok(())
            }

            pub fn try_unset_key(&mut self, key: &#writable_key_name) -> Result<(), #write_error_name> {
                match key {
                    #(#unset_arms)*
                    #fallback_branch
                };
                Ok(())
            }

            pub fn try_append_str(&mut self, reader: &#reader_name, key: &#writable_key_name, value: &str) -> Result<(), #write_error_name> {
                match key {
                    #(#append_arms)*
                    #fallback_branch
                };
                Ok(())
            }

            pub fn try_remove_str(&mut self, reader: &#reader_name, key: &#writable_key_name, value: &str) -> Result<(), #write_error_name> {
                match key {
                    #(#remove_arms)*
                    #fallback_branch
                };
                Ok(())
            }
        }
    }
}

/// Metadata for tracking sub-field information
#[derive(Clone, Debug)]
struct SubFieldInfo {
    type_name: syn::Ident,
}

/// A configuration key that is stored in an enum variant
///
/// The macro generates e.g. `ReadableKey` to list the variants
struct ConfigurationKey {
    /// e.g. `C8yUrl(Option<String>)`
    enum_variant: syn::Variant,
    /// An example of each field, with any multi-value keys set to `None`
    iter_field: syn::Expr,
    /// e.g. `C8yUrl(key0)`
    match_read_write: syn::Pat,
    /// e.g. `C8yUrl(_)`
    match_shape: syn::Pat,
    /// An if statement for extracting the multi field names out of value using a Regex
    ///
    /// This takes the string being matched using the identifier `value`, and binds `captures` if
    /// to [Regex::captures] if the string matches the key in question. The captures can be read
    /// using `captures.get(n)` where n is 1 for the first multi field, 2 for the second, etc.
    /// If the user is using a single configuration inside the multi field, the relevant capture will
    /// be `None`.
    ///
    /// If the field is not a "multi" field, `regex_parser` will be set to `None`
    regex_parser: Option<syn::ExprIf>,
    /// The variable names assigned to the multi fields within this configuration
    field_names: Vec<syn::Ident>,
    /// Format strings for each field e.g.
    /// ```compile_fail
    /// vec![
    ///     (C8yUrl(None), Cow::Borrowed("c8y.url")),
    ///     (C8yTopicPrefix(None), Cow::Borrowed("c8y.topic_prefix")),
    ///     (C8yUrl(Some(c8y_name)), Cow::Owned(format!("c8y.{c8y_name}.url"))),
    ///     (C8yTopicPrefix(Some(c8y_name)), Cow::Owned(format!("c8y.{c8y_name}.topic_prefix"))),
    /// ]
    /// ```
    formatters: Vec<(syn::Pat, syn::Expr)>,

    insert_profiles: Vec<(syn::Pat, syn::Expr)>,
    doc_comment: Option<String>,
    /// If this is a sub-field key, contains metadata about the sub-field
    sub_field_info: Option<SubFieldInfo>,
    write_error: Option<String>,
}

fn ident_for(segments: &VecDeque<&FieldOrGroup>) -> syn::Ident {
    syn::Ident::new(
        &segments
            .iter()
            .map(|segment| segment.name().to_upper_camel_case())
            .collect::<String>(),
        segments.iter().last().unwrap().ident().span(),
    )
}

fn enum_variant(segments: &VecDeque<&FieldOrGroup>) -> ConfigurationKey {
    let ident = ident_for(segments);
    let count_multi = segments
        .iter()
        .filter(|fog| matches!(fog, FieldOrGroup::Multi(_)))
        .count();
    let key_str = segments
        .iter()
        .map(|segment| segment.name())
        .interleave(std::iter::repeat_n(Cow::Borrowed("."), segments.len() - 1))
        .collect::<String>();
    if count_multi > 0 {
        let opt_strs = std::iter::repeat_n::<syn::Type>(parse_quote!(Option<String>), count_multi);
        let enum_variant = parse_quote_spanned!(ident.span()=> #ident(#(#opt_strs),*));
        let nones = std::iter::repeat_n::<syn::Path>(parse_quote!(None), count_multi);
        let iter_field = parse_quote_spanned!(ident.span()=> #ident(#(#nones),*));
        let field_names = SequentialIdGenerator::default()
            .take(count_multi)
            .collect::<Vec<_>>();
        let match_read_write = parse_quote_spanned!(ident.span()=> #ident(#(#field_names),*));
        let underscores = UnderscoreIdGenerator.take(count_multi);
        let match_shape = parse_quote_spanned!(ident.span()=> #ident(#(#underscores),*));
        let re = segments
            .iter()
            .map(|fog| match fog {
                FieldOrGroup::Multi(m) => {
                    format!("{}(?:[\\._]profiles[\\._]([^\\.]+))?", m.name())
                }
                FieldOrGroup::Field(f) => f.name().to_string(),
                FieldOrGroup::Group(g) => g.name().to_string(),
            })
            .collect::<Vec<_>>()
            .join("[\\._]");
        let re = format!("^{re}$");
        let regex_parser = parse_quote_spanned!(ident.span()=> if let Some(captures) = ::regex::Regex::new(#re).unwrap().captures(value) {});
        let formatters = field_names
            .iter()
            .map(|name| [parse_quote!(None), parse_quote!(Some(#name))])
            .multi_cartesian_product()
            .enumerate()
            .map(|(i, options): (_, Vec<syn::Pat>)| {
                if i == 0 {
                    (
                        parse_quote!(#ident(#(#options),*)),
                        parse_quote!(::std::borrow::Cow::Borrowed(#key_str)),
                    )
                } else {
                    let none: syn::Pat = parse_quote!(None);
                    let mut idents = field_names.iter().zip(options.iter());
                    let format_str = segments
                        .iter()
                        .map(|segment| match segment {
                            FieldOrGroup::Multi(m) => {
                                let (binding, opt) = idents.next().unwrap();
                                if *opt == none {
                                    m.ident.to_string()
                                } else {
                                    format!("{}.profiles.{{{}}}", m.ident, binding)
                                }
                            }
                            FieldOrGroup::Group(g) => g.name().to_string(),
                            FieldOrGroup::Field(f) => f.name().to_string(),
                        })
                        .interleave(std::iter::repeat_n(".".to_owned(), segments.len() - 1))
                        .collect::<String>();
                    (
                        parse_quote!(#ident(#(#options),*)),
                        parse_quote!(::std::borrow::Cow::Owned(format!(#format_str))),
                    )
                }
            })
            .collect();
        let insert_profiles = field_names
            .iter()
            .map(|_| [parse_quote!(None), parse_quote!(Some(_))])
            .multi_cartesian_product()
            .enumerate()
            .map(|(i, options): (_, Vec<syn::Pat>)| {
                if i == 0 {
                    (
                        parse_quote!(Self::#ident(#(#options),*)),
                        parse_quote!(Ok(Self::#ident(Some(profile.into())))),
                    )
                } else {
                    (
                        parse_quote!(c @ Self::#ident(#(#options),*)),
                        parse_quote!(::anyhow::bail!(
                            "Multiple profiles selected from the arguments {c} and --profile {profile}"
                        )),
                    )
                }
            })
            .collect();
        ConfigurationKey {
            enum_variant,
            iter_field,
            match_shape,
            match_read_write,
            regex_parser: Some(regex_parser),
            field_names,
            formatters,
            insert_profiles,
            doc_comment: segments.iter().last().unwrap().doc(),
            sub_field_info: None,
            write_error: (|| {
                Some(
                    segments
                        .back()?
                        .field()?
                        .read_only()?
                        .readonly
                        .write_error
                        .clone(),
                )
            })(),
        }
    } else {
        ConfigurationKey {
            enum_variant: parse_quote!(#ident),
            iter_field: parse_quote!(#ident),
            match_read_write: parse_quote!(#ident),
            match_shape: parse_quote!(#ident),
            regex_parser: None,
            field_names: vec![],
            formatters: vec![(
                parse_quote!(#ident),
                parse_quote!(::std::borrow::Cow::Borrowed(#key_str)),
            )],
            insert_profiles: vec![],
            doc_comment: segments.back().unwrap().doc(),
            sub_field_info: None,
            write_error: (|| {
                Some(
                    segments
                        .back()?
                        .field()?
                        .read_only()?
                        .readonly
                        .write_error
                        .clone(),
                )
            })(),
        }
    }
}

/// Which type (`TEdgeConfigDto`/`TEdgeConfigReader`) we should generate a key
/// enum for
///
/// For the CLI, we allow keys based on the reader, so values like
/// `config.version` aren't configured by or visible to the user. For detecting
/// unknown keys in the TOML file however, we need to generate keys from the
/// DTO since `config.version` is allowed in the TOML file.
#[derive(Debug, Clone, Copy)]
enum Mode {
    Dto,
    Reader,
}

impl Mode {
    pub fn skip(self, item: &FieldOrGroup) -> bool {
        match self {
            Self::Dto => item.dto_skip(),
            Self::Reader => item.reader().skip,
        }
    }
}

/// Generates a list of the toml paths for each of the keys in the provided
/// configuration
fn configuration_paths_from(items: &[FieldOrGroup], mode: Mode) -> Vec<VecDeque<&FieldOrGroup>> {
    let mut res = vec![];
    for item in items.iter().filter(|item| !mode.skip(item)) {
        match item {
            FieldOrGroup::Field(_) => res.push(VecDeque::from([item])),
            FieldOrGroup::Group(group) | FieldOrGroup::Multi(group) => {
                for mut fields in configuration_paths_from(&group.contents, mode) {
                    fields.push_front(item);
                    res.push(fields);
                }
            }
        }
    }
    res
}

/// Checks if the field for the given path is read write
fn is_read_write(path: &VecDeque<&FieldOrGroup>) -> bool {
    matches!(
        path.back(), // the field
        Some(FieldOrGroup::Field(ConfigurableField::ReadWrite(_))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use prettyplease::unparse;
    use syn::ImplItem;
    use syn::ItemImpl;
    use test_case::test_case;

    #[test]
    fn output_parses() {
        syn::parse2::<syn::File>(generate_writable_keys(&ctx(), &[])).unwrap();
    }

    #[test]
    fn output_parses_for_multi() {
        let input: crate::input::Configuration = parse_quote! {
            #[tedge_config(multi)]
            c8y: {
                url: String
            }
        };
        syn::parse2::<syn::File>(generate_writable_keys(&ctx(), &input.groups)).unwrap();
    }

    #[test]
    fn from_str_does_not_generate_regex_matches_for_normal_fields() {
        let input: crate::input::Configuration = parse_quote!(
            c8y: {
                url: String,
            }
        );
        let gen_ctx = gen_ctx();
        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let c = configuration_strings(paths.iter(), FilterRule::None, &gen_ctx.readable_key_name);
        let generated = generate_fromstr(
            &gen_ctx.readable_key_name,
            &c,
            parse_quote!(_ => unimplemented!("just a test, no error handling")),
            &gen_ctx,
        );
        let expected = parse_quote!(
            impl ::std::str::FromStr for ReadableKey {
                type Err = ParseKeyError;
                fn from_str(value: &str) -> Result<Self, Self::Err> {
                    #[deny(unreachable_patterns)]
                    let res = match replace_aliases(value.to_owned()).replace(".", "_").as_str() {
                        "c8y_url" => {
                            if value != "c8y.url" {
                                warn_about_deprecated_key(value.to_owned(), "c8y.url");
                            }
                            return Ok(Self::C8yUrl);
                        },
                        _ => unimplemented!("just a test, no error handling"),
                    };
                    res
                }
            }
        );
        pretty_assertions::assert_eq!(
            prettyplease::unparse(&syn::parse2(generated).unwrap()),
            prettyplease::unparse(&expected)
        );
    }

    /// The regex generated for `c8y.url`
    ///
    /// This is used to verify both the output of the macro matches this regex
    /// and that the regex itself functions as intended
    const C8Y_URL_REGEX: &str = "^c8y(?:[\\._]profiles[\\._]([^\\.]+))?[\\._]url$";

    /// The regex generated for `mapper.ty` with multi-field profiles
    const MAPPER_TY_REGEX: &str = "^mapper(?:[\\._]profiles[\\._]([^\\.]+))?[\\._]type$";

    /// The regex generated for `mapper.c8y.*` (sub-field with profiles)
    /// Captures: (1) profile name, (2) remainder after the sub-field prefix
    const MAPPER_TY_C8Y_REGEX: &str =
        "^mapper(?:[\\._]profiles[\\._]([^\\.]+))?[\\._]c8y[\\._](.+)$";

    #[test_case("c8y.url", None; "with no profile specified")]
    #[test_case("c8y.profiles.name.url", Some("name"); "with profile toml syntax")]
    #[test_case("c8y_profiles_name_url", Some("name"); "with environment variable profile")]
    #[test_case("c8y_profiles_name_underscore_url", Some("name_underscore"); "with environment variable underscore profile")]
    fn regex_matches(input: &str, output: Option<&str>) {
        let re = regex::Regex::new(C8Y_URL_REGEX).unwrap();
        assert_eq!(
            re.captures(input).unwrap().get(1).map(|s| s.as_str()),
            output
        );
    }

    #[test_case("not.c8y.url"; "with an invalid prefix")]
    #[test_case("c8y.url.something"; "with an invalid suffix")]
    #[test_case("c8y.profiles.multiple.profile.sections.url"; "with an invalid profile name")]
    fn regex_fails(input: &str) {
        let re = regex::Regex::new(C8Y_URL_REGEX).unwrap();
        assert!(re.captures(input).is_none());
    }

    #[test_case("mapper.c8y.instance", (None, "instance"); "with no profile")]
    #[test_case("mapper.profiles.myprofile.c8y.instance", (Some("myprofile"), "instance"); "with profile toml syntax")]
    #[test_case("mapper_profiles_myprofile_c8y_instance", (Some("myprofile"), "instance"); "with environment variable syntax")]
    fn sub_field_regex_matches(input: &str, (profile, remainder): (Option<&str>, &str)) {
        let re = regex::Regex::new(MAPPER_TY_C8Y_REGEX).unwrap();
        let captures = re.captures(input).unwrap();
        assert_eq!(
            captures.get(1).map(|s| s.as_str()),
            profile,
            "Profile capture should match"
        );
        assert_eq!(
            captures.get(2).map(|s| s.as_str()),
            Some(remainder),
            "Remainder capture should match"
        );
    }

    #[test_case("mapper.type.custom.field"; "with custom sub-field instead of c8y")]
    #[test_case("mapper.type"; "with no sub-field")]
    #[test_case("mapper.c8y"; "with sub-field but no remainder")]
    fn sub_field_regex_fails(input: &str) {
        let re = regex::Regex::new(MAPPER_TY_C8Y_REGEX).unwrap();
        assert!(re.captures(input).is_none());
    }

    #[test]
    fn from_str_generates_regex_matches_for_multi_fields() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            c8y: {
                url: String,
            }
        );
        let gen_ctx = gen_ctx();
        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let c = configuration_strings(paths.iter(), FilterRule::None, &gen_ctx.readable_key_name);
        let generated = generate_fromstr(
            &gen_ctx.readable_key_name,
            &c,
            parse_quote!(_ => unimplemented!("just a test, no error handling")),
            &gen_ctx,
        );
        let expected = parse_quote!(
            impl ::std::str::FromStr for ReadableKey {
                type Err = ParseKeyError;
                fn from_str(value: &str) -> Result<Self, Self::Err> {
                    #[deny(unreachable_patterns)]
                    let res = match replace_aliases(value.to_owned()).replace(".", "_").as_str() {
                        "c8y_url" => {
                            if value != "c8y.url" {
                                warn_about_deprecated_key(value.to_owned(), "c8y.url");
                            }
                            return Ok(Self::C8yUrl(None));
                        },
                        _ => unimplemented!("just a test, no error handling"),
                    };
                    if let Some(captures) = ::regex::Regex::new(#C8Y_URL_REGEX).unwrap().captures(value) {
                        let key0 = captures.get(1usize).map(|re_match| re_match.as_str().to_owned());
                        return Ok(Self::C8yUrl(key0));
                    };
                    res
                }
            }
        );
        pretty_assertions::assert_eq!(
            prettyplease::unparse(&syn::parse2(generated).unwrap()),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn iteration_of_multi_fields() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            c8y: {
                url: String,
                #[tedge_config(multi)]
                something: {
                    test: u16,
                }
            }
        );
        let mut paths = configuration_paths_from(&input.groups, Mode::Reader);
        let paths = paths.iter_mut().map(|vd| &*vd.make_contiguous());
        let generated = key_iterators(
            &parse_quote!(TEdgeConfigReader),
            &parse_quote!(ReadableKey),
            &parse_quote!(readable_keys),
            &paths.collect::<Vec<_>>(),
            "",
            &[],
        );
        let expected = parse_quote! {
            impl TEdgeConfigReader {
                pub fn readable_keys(&self) -> impl Iterator<Item = ReadableKey> + '_ {
                    let c8y_keys = self.c8y.keys().map(|k| Some(k?.to_string())).collect::<Vec<_>>();
                    let c8y_keys = c8y_keys
                        .into_iter()
                        .flat_map(|c8y| self.c8y.try_get(c8y.as_deref()).unwrap().readable_keys(c8y));

                    c8y_keys
                }
            }

            impl TEdgeConfigReaderC8y {
                pub fn readable_keys(&self, c8y: Option<String>) -> impl Iterator<Item = ReadableKey> + '_ {
                    let something_keys = self.something.keys().map(|k| Some(k?.to_string())).collect::<Vec<_>>();
                    let something_keys = something_keys.into_iter().flat_map({
                        let c8y = c8y.clone();
                        move |something| {
                            self.something
                                .try_get(something.as_deref())
                                .unwrap()
                                .readable_keys(c8y.clone(), something)
                        }
                    });

                    [ReadableKey::C8yUrl(c8y.clone())].into_iter().chain(something_keys)
                }
            }

            impl TEdgeConfigReaderC8ySomething {
                pub fn readable_keys(
                    &self,
                    c8y: Option<String>,
                    something: Option<String>,
                ) -> impl Iterator<Item = ReadableKey> + '_ {
                    [ReadableKey::C8ySomethingTest(
                        c8y.clone(),
                        something.clone(),
                    )]
                    .into_iter()
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&syn::parse2(generated).unwrap()),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn iteration_of_non_multi_fields() {
        let input: crate::input::Configuration = parse_quote!(
            c8y: {
                url: String,
            }
        );
        let mut paths = configuration_paths_from(&input.groups, Mode::Reader);
        let paths = paths.iter_mut().map(|vd| &*vd.make_contiguous());
        let generated = key_iterators(
            &parse_quote!(TEdgeConfigReader),
            &parse_quote!(ReadableKey),
            &parse_quote!(readable_keys),
            &paths.collect::<Vec<_>>(),
            "",
            &[],
        );
        let expected = parse_quote! {
            impl TEdgeConfigReader {
                pub fn readable_keys(&self) -> impl Iterator<Item = ReadableKey> + '_ {
                    self.c8y.readable_keys()
                }
            }

            impl TEdgeConfigReaderC8y {
                pub fn readable_keys(&self) -> impl Iterator<Item = ReadableKey> + '_ {
                    [ReadableKey::C8yUrl].into_iter()
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&syn::parse2(generated).unwrap()),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn iteration_of_sub_fields_recurses_to_sub_config() {
        let input: crate::input::Configuration = parse_quote!(
            mapper: {
                enable: bool,

                #[tedge_config(rename = "type")]
                #[tedge_config(sub_fields = [C8y(C8y), Az(Az), Custom])]
                ty: MapperType,

                url: String,
            }
        );
        let mut paths = configuration_paths_from(&input.groups, Mode::Reader);
        let paths = paths.iter_mut().map(|vd| &*vd.make_contiguous());
        let generated = key_iterators(
            &parse_quote!(TEdgeConfigReader),
            &parse_quote!(ReadableKey),
            &parse_quote!(readable_keys),
            &paths.collect::<Vec<_>>(),
            "",
            &[],
        );
        let expected = parse_quote! {
            impl TEdgeConfigReader {
                pub fn readable_keys(&self) -> impl Iterator<Item = ReadableKey> + '_ {
                    self.mapper.readable_keys()
                }
            }

            impl TEdgeConfigReaderMapper {
                pub fn readable_keys(&self) -> impl Iterator<Item = ReadableKey> + '_ {
                    [ReadableKey::MapperEnable, ReadableKey::MapperType, ReadableKey::MapperUrl]
                        .into_iter()
                        .chain({
                            self.ty.or_none().into_iter().flat_map(move |ty| ty.readable_keys())
                        })
                }
            }

            impl MapperTypeReader {
                pub fn readable_keys(&self) -> Vec<ReadableKey> {
                    match self {
                        Self::C8y { c8y } => c8y.readable_keys().map(|inner_key| ReadableKey::MapperTypeC8y(inner_key)).collect(),
                        Self::Az { az } => az.readable_keys().map(|inner_key| ReadableKey::MapperTypeAz(inner_key)).collect(),
                        Self::Custom => Vec::new(),
                    }
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&syn::parse2(generated).unwrap()),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn iteration_of_multi_profile_sub_fields() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(rename = "type")]
                #[tedge_config(sub_fields = [C8y(C8y), Az(Az), Custom])]
                ty: MapperType,
            }
        );
        let mut paths = configuration_paths_from(&input.groups, Mode::Reader);
        let paths = paths.iter_mut().map(|vd| &*vd.make_contiguous());
        let generated = key_iterators(
            &parse_quote!(TEdgeConfigReader),
            &parse_quote!(ReadableKey),
            &parse_quote!(readable_keys),
            &paths.collect::<Vec<_>>(),
            "",
            &[],
        );
        let mut actual: syn::File = syn::parse2(generated).unwrap();
        actual.items.retain(|item| matches!(item, syn::Item::Impl(syn::ItemImpl { self_ty, .. }) if **self_ty == parse_quote!(TEdgeConfigReaderMapper)));
        let expected = parse_quote! {
            impl TEdgeConfigReaderMapper {
                pub fn readable_keys(&self, mapper: Option<String>) -> impl Iterator<Item = ReadableKey> + '_ {
                    [ReadableKey::MapperType(mapper.clone())]
                        .into_iter()
                        .chain({
                            let mapper = mapper.clone();
                            self.ty.or_none().into_iter().flat_map(move |ty| ty.readable_keys(mapper.clone()))
                        })
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&actual),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn iteration_of_empty_field_enum_is_an_empty_iterator() {
        let generated = key_iterators(
            &parse_quote!(TEdgeConfigReader),
            &parse_quote!(ReadableKey),
            &parse_quote!(readable_keys),
            &[],
            "",
            &[],
        );
        let expected = parse_quote! {
            impl TEdgeConfigReader {
                pub fn readable_keys(&self) -> impl Iterator<Item = ReadableKey> + '_ {
                    std::iter::empty()
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&syn::parse2(generated).unwrap()),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn iteration_of_non_multi_groups() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            c8y: {
                nested: {
                    field: String,
                }
            }
        );
        let mut paths = configuration_paths_from(&input.groups, Mode::Reader);
        let paths = paths.iter_mut().map(|vd| &*vd.make_contiguous());
        let generated = key_iterators(
            &parse_quote!(TEdgeConfigReader),
            &parse_quote!(ReadableKey),
            &parse_quote!(readable_keys),
            &paths.collect::<Vec<_>>(),
            "",
            &[],
        );
        let expected = parse_quote! {
            impl TEdgeConfigReader {
                pub fn readable_keys(&self) -> impl Iterator<Item = ReadableKey> + '_ {
                    let c8y_keys = self.c8y.keys().map(|k| Some(k?.to_string())).collect::<Vec<_>>();
                    let c8y_keys = c8y_keys
                        .into_iter()
                        .flat_map(|c8y| self.c8y.try_get(c8y.as_deref()).unwrap().readable_keys(c8y));

                    c8y_keys
                }
            }

            impl TEdgeConfigReaderC8y {
                pub fn readable_keys(&self, c8y: Option<String>) -> impl Iterator<Item = ReadableKey> + '_ {
                    self.nested.readable_keys(c8y.clone())
                }
            }

            impl TEdgeConfigReaderC8yNested {
                pub fn readable_keys(
                    &self,
                    c8y: Option<String>,
                ) -> impl Iterator<Item = ReadableKey> + '_ {
                    [ReadableKey::C8yNestedField(
                        c8y.clone(),
                    )]
                    .into_iter()
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&syn::parse2(generated).unwrap()),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn impl_for_simple_group() {
        let input: crate::input::Configuration = parse_quote!(
            c8y: {
                url: String,
            }
        );
        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let config_keys =
            configuration_strings(paths.iter(), FilterRule::None, &gen_ctx().readable_key_name);
        let impl_block = retain_fn(keys_enum_impl_block(&config_keys), "to_cow_str");

        let expected = parse_quote! {
            impl ReadableKey {
                pub fn to_cow_str(&self) -> ::std::borrow::Cow<'static, str> {
                    match self {
                        Self::C8yUrl => ::std::borrow::Cow::Borrowed("c8y.url"),
                    }
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#impl_block)),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn impl_ignores_skipped_groups() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(reader(skip))]
            config: {
                version: String,
            },

            c8y: {
                url: String,
            }
        );
        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let config_keys =
            configuration_strings(paths.iter(), FilterRule::None, &gen_ctx().readable_key_name);
        let impl_block = retain_fn(keys_enum_impl_block(&config_keys), "to_cow_str");

        let expected = parse_quote! {
            impl ReadableKey {
                pub fn to_cow_str(&self) -> ::std::borrow::Cow<'static, str> {
                    match self {
                        Self::C8yUrl => ::std::borrow::Cow::Borrowed("c8y.url"),
                    }
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#impl_block)),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn impl_for_multi() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            c8y: {
                url: String,
                #[tedge_config(rename = "type")]
                ty: String,
            }
        );
        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let config_keys =
            configuration_strings(paths.iter(), FilterRule::None, &gen_ctx().readable_key_name);
        let impl_block = retain_fn(keys_enum_impl_block(&config_keys), "to_cow_str");

        let expected = parse_quote! {
            impl ReadableKey {
                pub fn to_cow_str(&self) -> ::std::borrow::Cow<'static, str> {
                    match self {
                        Self::C8yUrl(None) => ::std::borrow::Cow::Borrowed("c8y.url"),
                        Self::C8yUrl(Some(key0)) => ::std::borrow::Cow::Owned(format!("c8y.profiles.{key0}.url")),
                        Self::C8yType(None) => ::std::borrow::Cow::Borrowed("c8y.type"),
                        Self::C8yType(Some(key0)) => ::std::borrow::Cow::Owned(format!("c8y.profiles.{key0}.type")),
                    }
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#impl_block)),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn impl_for_nested_multi() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            top: {
                #[tedge_config(multi)]
                nested: {
                    field: String,
                }
            }
        );
        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let config_keys =
            configuration_strings(paths.iter(), FilterRule::None, &gen_ctx().readable_key_name);
        let impl_block = retain_fn(keys_enum_impl_block(&config_keys), "to_cow_str");

        let expected = parse_quote! {
            impl ReadableKey {
                pub fn to_cow_str(&self) -> ::std::borrow::Cow<'static, str> {
                    match self {
                        Self::TopNestedField(None, None) => ::std::borrow::Cow::Borrowed("top.nested.field"),
                        Self::TopNestedField(None, Some(key1)) => ::std::borrow::Cow::Owned(format!("top.nested.profiles.{key1}.field")),
                        Self::TopNestedField(Some(key0), None) => ::std::borrow::Cow::Owned(format!("top.profiles.{key0}.nested.field")),
                        Self::TopNestedField(Some(key0), Some(key1)) => ::std::borrow::Cow::Owned(format!("top.profiles.{key0}.nested.profiles.{key1}.field")),
                    }
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#impl_block)),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn impl_try_with_profile() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            c8y: {
                url: String,

                availability: {
                    interval: i32,
                }
            },

            sudo: {
                enable: bool,
            },
        );
        let gen_ctx = gen_ctx();
        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let config_keys =
            configuration_strings(paths.iter(), FilterRule::None, &gen_ctx.readable_key_name);
        let impl_block = retain_fn(keys_enum_impl_block(&config_keys), "try_with_profile");

        let expected = parse_quote! {
            impl ReadableKey {
                pub fn try_with_profile(self, profile: ProfileName) -> ::anyhow::Result<Self> {
                    match self {
                        Self::C8yUrl(None) => Ok(Self::C8yUrl(Some(profile.into()))),
                        c @ Self::C8yUrl(Some(_)) => ::anyhow::bail!("Multiple profiles selected from the arguments {c} and --profile {profile}"),
                        Self::C8yAvailabilityInterval(None) => Ok(Self::C8yAvailabilityInterval(Some(profile.into()))),
                        c @ Self::C8yAvailabilityInterval(Some(_)) => ::anyhow::bail!("Multiple profiles selected from the arguments {c} and --profile {profile}"),
                        other => ::anyhow::bail!("You've supplied a profile, but {other} is not a profiled configuration"),
                    }
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#impl_block)),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn impl_try_unset_key_calls_multi_dto_cleanup() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            c8y: {
                url: String,

                #[tedge_config(multi)]
                nested: {
                    field: bool,
                }
            },

            sudo: {
                enable: bool,
            },
        );
        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let writers = generate_string_writers(&paths, &gen_ctx());
        let impl_dto_block = syn::parse2(writers).unwrap();
        let impl_dto_block = retain_fn(impl_dto_block, "try_unset_key");

        let expected = parse_quote! {
            impl TEdgeConfigDto {
                pub fn try_unset_key(&mut self, key: &WritableKey) -> Result<(), WriteError> {
                    match key {
                        WritableKey::C8yUrl(key0) => {
                            self.c8y.try_get_mut(key0.as_deref(), "c8y")?.url = None;
                            self.c8y.remove_if_empty(key0.as_deref());
                        }
                        WritableKey::C8yNestedField(key0, key1) => {
                            self.c8y.try_get_mut(key0.as_deref(), "c8y")?.nested.try_get_mut(key1.as_deref(), "c8y.nested")?.field = None;
                            // The fields should be removed from deepest to shallowest
                            self.c8y.try_get_mut(key0.as_deref(), "c8y")?.nested.remove_if_empty(key1.as_deref());
                            self.c8y.remove_if_empty(key0.as_deref());
                        }
                        WritableKey::SudoEnable => {
                            self.sudo.enable = None;
                        },
                    };
                    Ok(())
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#impl_dto_block)),
            prettyplease::unparse(&expected)
        )
    }

    #[test]
    fn impl_try_append_calls_method_for_current_value() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            c8y: {
                device: {
                    #[tedge_config(reader(function = "device_id"))]
                    id: String,
                },
            }
        );
        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let writers = generate_string_writers(&paths, &gen_ctx());
        let impl_dto_block = syn::parse2(writers).unwrap();
        let impl_dto_block = retain_fn(impl_dto_block, "try_append_str");

        let expected = parse_quote! {
            impl TEdgeConfigDto {
                pub fn try_append_str(&mut self, reader: &TEdgeConfigReader, key: &WritableKey, value: &str) -> Result<(), WriteError> {
                    match key {
                        #[allow(clippy::useless_conversion)]
                        WritableKey::C8yDeviceId(key0) => {
                            self.c8y.try_get_mut(key0.as_deref(), "c8y")?.device.id = <String as AppendRemoveItem>::append(
                                Some(reader.c8y.try_get(key0.as_deref())?.device.id()),
                                value
                                    .parse::<String>()
                                    .map(<String>::from)
                                    .map_err(|e| WriteError::ParseValue(Box::new(e)))?,
                            );
                        }
                    };
                    Ok(())
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#impl_dto_block)),
            prettyplease::unparse(&expected)
        )
    }

    #[test]
    fn fromstr_is_rename_aware_for_profiled_configurations() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(rename = "type")]
                ty: String,
            },
            device: {
                #[tedge_config(rename = "type")]
                ty: String,
            },
        );
        let gen_ctx = gen_ctx();
        let dto_paths = configuration_paths_from(&input.groups, Mode::Dto);
        let dto_keys =
            configuration_strings(dto_paths.iter(), FilterRule::None, &gen_ctx.dto_key_name);
        let writers = generate_fromstr_writable(&parse_quote!(DtoKey), &dto_keys, &gen_ctx);
        let impl_dto_block = syn::parse2(writers).unwrap();

        let expected = parse_quote! {
            impl ::std::str::FromStr for DtoKey {
                type Err = ParseKeyError;

                fn from_str(value: &str) -> Result<Self, Self::Err> {
                    #[deny(unreachable_patterns)]
                    let res = match replace_aliases(value.to_owned()).replace(".", "_").as_str() {
                        "mapper_type" => {
                            if value != "mapper.type" {
                                warn_about_deprecated_key(value.to_owned(), "mapper.type");
                            }
                            return Ok(Self::MapperType(None));
                        }
                        "device_type" => {
                            if value != "device.type" {
                                warn_about_deprecated_key(value.to_owned(), "device.type");
                            }
                            return Ok(Self::DeviceType);
                        }
                        _ => {
                            if let Ok(key) = <ReadOnlyKey as ::std::str::FromStr>::from_str(value) {
                                Err(ParseKeyError::ReadOnly(key))
                            } else {
                                Err(ParseKeyError::Unrecognised(value.to_owned()))
                            }
                        }
                    };
                    if let Some(captures) = ::regex::Regex::new(
                            "^mapper(?:[\\._]profiles[\\._]([^\\.]+))?[\\._]type$",
                        )
                        .unwrap()
                        .captures(value)
                    {
                        let key0 = captures.get(1usize).map(|re_match| re_match.as_str().to_owned());
                        return Ok(Self::MapperType(key0));
                    }
                    res
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&impl_dto_block),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn writable_keys_includes_ability_to_set_sub_fields() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y), Az(Az), Custom])]
                #[tedge_config(rename = "type")]
                ty: MapperType,
            },
        );
        let writers = generate_writable_keys(&ctx(), &input.groups);
        let mut actual: syn::File = syn::parse2(writers).unwrap();
        actual.items.retain(|item| matches!(item, syn::Item::Enum(syn::ItemEnum { ident, .. }) if !ident.to_string().ends_with("Error")));
        actual.items.iter_mut().for_each(|item| {
            if let syn::Item::Enum(enumm) = item {
                enumm.attrs.retain(|attr| !is_doc_comment(attr));
                for variant in &mut enumm.variants {
                    variant.attrs.retain(|attr| !is_doc_comment(attr));
                }
            }
        });

        let expected = parse_quote! {
            #[derive(Clone, Debug, PartialEq, Eq)]
            #[non_exhaustive]
            #[allow(unused)]
            pub enum ReadableKey {
                MapperType(Option<String>),
                MapperTypeC8y(Option<String>, C8yReadableKey),
                MapperTypeAz(Option<String>, AzReadableKey),
            }

            #[derive(Clone, Debug, PartialEq, Eq)]
            #[non_exhaustive]
            #[allow(unused)]
            pub enum ReadOnlyKey {
                MapperTypeC8y(Option<String>, C8yReadOnlyKey),
                MapperTypeAz(Option<String>, AzReadOnlyKey),
            }

            #[derive(Clone, Debug, PartialEq, Eq)]
            #[non_exhaustive]
            #[allow(unused)]
            pub enum WritableKey {
                MapperType(Option<String>),
                MapperTypeC8y(Option<String>, C8yWritableKey),
                MapperTypeAz(Option<String>, AzWritableKey),
            }

            #[derive(Clone, Debug, PartialEq, Eq)]
            #[non_exhaustive]
            #[allow(unused)]
            pub enum DtoKey {
                MapperType(Option<String>),
                MapperTypeC8y(Option<String>, C8yDtoKey),
                MapperTypeAz(Option<String>, AzDtoKey),
            }
        };

        pretty_assertions::assert_eq!(unparse(&actual), unparse(&expected));
    }

    #[test]
    fn write_error_method_recurses_to_sub_fields() {
        let input: crate::input::Configuration = parse_quote!(
            device: {
                #[tedge_config(readonly(write_error = "An example error message", function = "device_id"))]
                id: String
            },
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y), Az(Az), Custom])]
                #[tedge_config(rename = "type")]
                ty: MapperType,
            },
        );
        let actual = generate_writable_keys(&ctx(), &input.groups);
        let mut actual: syn::File = syn::parse2(actual).unwrap();
        actual.items = actual
            .items
            .into_iter()
            .filter_map(|mut item| {
                if let syn::Item::Impl(i) = &mut item {
                    i.items.retain(
                        |item| matches!(item, syn::ImplItem::Fn(f) if f.sig.ident == "write_error"),
                    );
                    if !i.items.is_empty() {
                        Some(item)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        let expected: syn::File = parse_quote!(
            impl ReadOnlyKey {
                fn write_error(&self) -> &'static str {
                    match self {
                        Self::DeviceId => "An example error message",
                        #[allow(unused)]
                        Self::MapperTypeC8y(key0, sub_key) => sub_key.write_error(),
                        #[allow(unused)]
                        Self::MapperTypeAz(key0, sub_key) => sub_key.write_error(),
                    }
                }
            }
        );

        pretty_assertions::assert_eq!(unparse(&actual), unparse(&expected));
    }

    #[test]
    fn sub_fields_dont_trigger_nested_profiles_error() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y), Az(Az), Custom])]
                #[tedge_config(rename = "type")]
                ty: MapperType,
            },
        );
        let gen_ctx = gen_ctx();
        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let config_keys =
            configuration_strings(paths.iter(), FilterRule::None, &gen_ctx.readable_key_name);
        let impl_block = retain_fn(keys_enum_impl_block(&config_keys), "try_with_profile");

        let expected = parse_quote! {
            impl ReadableKey {
                pub fn try_with_profile(self, profile: ProfileName) -> ::anyhow::Result<Self> {
                    match self {
                        Self::MapperType(None) => Ok(Self::MapperType(Some(profile.into()))),
                        c @ Self::MapperType(Some(_)) => ::anyhow::bail!("Multiple profiles selected from the arguments {c} and --profile {profile}"),
                        other => ::anyhow::bail!("You've supplied a profile, but {other} is not a profiled configuration"),
                    }
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#impl_block)),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn read_string_handles_sub_field_keys() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y), Custom])]
                #[tedge_config(rename = "type")]
                ty: MapperType,
            },
        );

        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let readers = generate_string_readers(&paths, &gen_ctx());
        let impl_block: syn::ItemImpl = syn::parse2(readers).unwrap();
        let actual = retain_fn(impl_block, "read_string");

        let expected = parse_quote! {
            impl TEdgeConfigReader {
                pub fn read_string(&self, key: &ReadableKey) -> Result<String, ReadError> {
                    match key {
                        ReadableKey::MapperType(key0) => Ok(self.mapper.try_get(key0.as_deref())?.ty.or_config_not_set()?.to_string()),
                        ReadableKey::MapperTypeC8y(key0, sub_key) => {
                            let mapper_ty = &self.mapper.try_get(key0.as_deref())?.ty.or_config_not_set()?;
                            match mapper_ty {
                                MapperTypeReader::C8y { c8y } => {
                                    c8y.read_string(sub_key)
                                }
                                _ => unreachable!("Attempted to read C8y sub-field from non-C8y variant"),
                            }
                        }
                    }
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#actual)),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn write_string_handles_sub_field_keys() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y), Custom])]
                ty: MapperType,
            },
        );

        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let writers = generate_string_writers(&paths, &gen_ctx());
        let impl_block: syn::ItemImpl = syn::parse2(writers).unwrap();
        let actual = retain_fn(impl_block, "try_update_str");

        let expected = parse_quote! {
            impl TEdgeConfigDto {
                pub fn try_update_str(&mut self, key: &WritableKey, value: &str) -> Result<(), WriteError> {
                    match key {
                        #[allow(clippy::useless_conversion)]
                        WritableKey::MapperTy(key0) => self.mapper.try_get_mut(key0.as_deref(), "mapper")?.ty = Some(value.parse::<MapperTypeDto>().map(<MapperTypeDto>::from).map_err(|e| WriteError::ParseValue(Box::new(e)))?),
                        WritableKey::MapperTyC8y(key0, sub_key) => {
                            let mapper = self.mapper.try_get_mut(key0.as_deref(), "mapper")?;
                            let mapper_ty = mapper.ty.get_or_insert_with(|| MapperTypeDto::C8y { c8y: C8yDto::default() });
                            if let MapperTypeDto::C8y { c8y } = mapper_ty {
                                c8y.try_update_str(sub_key, value)?;
                            } else {
                                return Err(WriteError::SuperFieldWrongValue {
                                    target: key.clone(),
                                    parent: WritableKey::MapperTy(key0.clone()),
                                    parent_expected: "c8y".to_string(),
                                    parent_actual: mapper_ty.to_string(),
                                });
                            }
                        }
                    };
                    Ok(())
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#actual)),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn take_value_from_handles_sub_field_keys() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y), Custom])]
                ty: MapperType,
            },
        );

        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let writers = generate_string_writers(&paths, &gen_ctx());
        let impl_block: syn::ItemImpl = syn::parse2(writers).unwrap();
        let actual = retain_fn(impl_block, "take_value_from");

        let expected = parse_quote! {
            impl TEdgeConfigDto {
                pub(crate) fn take_value_from(&mut self, other: &mut TEdgeConfigDto, key: &WritableKey) -> Result<(), WriteError> {
                    match key {
                        WritableKey::MapperTy(key0) => {
                            self.mapper.try_get_mut(key0.as_deref(), "mapper")?.ty = other
                                .mapper
                                .try_get_mut(key0.as_deref(), "mapper")?
                                .ty
                                .take();
                        }
                        WritableKey::MapperTyC8y(key0, sub_key) => {
                            let mapper = self.mapper.try_get_mut(key0.as_deref(), "mapper")?;
                            let mapper_ty = mapper.ty.get_or_insert_with(|| MapperTypeDto::C8y { c8y: C8yDto::default() });
                            if let MapperTypeDto::C8y { c8y } = mapper_ty {
                                let other_mapper = other.mapper.try_get_mut(key0.as_deref(), "mapper")?;
                                let other_mapper_ty = &mut other_mapper.ty;
                                if let Some(MapperTypeDto::C8y { c8y: ref mut other_c8y }) = other_mapper_ty {
                                    c8y.take_value_from(other_c8y, sub_key)?;
                                }
                            } else {
                                return Err(WriteError::SuperFieldWrongValue {
                                    target: key.clone(),
                                    parent: WritableKey::MapperTy(key0.clone()),
                                    parent_expected: "c8y".to_string(),
                                    parent_actual: mapper_ty.to_string(),
                                });
                            }
                        }
                    };
                    Ok(())
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#actual)),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn try_unset_key_handles_sub_field_keys() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y), Custom])]
                ty: MapperType,
            },
        );

        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let writers = generate_string_writers(&paths, &gen_ctx());
        let impl_block: syn::ItemImpl = syn::parse2(writers).unwrap();
        let actual = retain_fn(impl_block, "try_unset_key");

        let expected = parse_quote! {
            impl TEdgeConfigDto {
                pub fn try_unset_key(&mut self, key: &WritableKey) -> Result<(), WriteError> {
                    match key {
                        WritableKey::MapperTy(key0) => {
                            self.mapper.try_get_mut(key0.as_deref(), "mapper")?.ty = None;
                            self.mapper.remove_if_empty(key0.as_deref());
                        }
                        WritableKey::MapperTyC8y(key0, sub_key) => {
                            let mapper = self.mapper.try_get_mut(key0.as_deref(), "mapper")?;
                            let mapper_ty = mapper.ty.get_or_insert_with(|| MapperTypeDto::C8y { c8y: C8yDto::default() });
                            if let MapperTypeDto::C8y { c8y } = mapper_ty {
                                c8y.try_unset_key(sub_key)?;
                            } else {
                                return Err(WriteError::SuperFieldWrongValue {
                                    target: key.clone(),
                                    parent: WritableKey::MapperTy(key0.clone()),
                                    parent_expected: "c8y".to_string(),
                                    parent_actual: mapper_ty.to_string(),
                                });
                            }
                        }
                    };
                    Ok(())
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#actual)),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn try_append_str_handles_sub_field_keys() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y), Custom])]
                ty: MapperType,
            },
        );
        let gen_ctx = gen_ctx();

        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let writers = generate_string_writers(&paths, &gen_ctx);
        let impl_block: syn::ItemImpl = syn::parse2(writers).unwrap();
        let actual = retain_fn(impl_block, "try_append_str");

        let expected = parse_quote! {
            impl TEdgeConfigDto {
                pub fn try_append_str(&mut self, reader: &TEdgeConfigReader, key: &WritableKey, value: &str) -> Result<(), WriteError> {
                    match key {
                        #[allow(clippy::useless_conversion)]
                        WritableKey::MapperTy(key0) => {
                            self.mapper.try_get_mut(key0.as_deref(), "mapper")?.ty = <MapperTypeDto as AppendRemoveItem>::append(
                                self.mapper.try_get_mut(key0.as_deref(), "mapper")?.ty.take(),
                                value
                                    .parse::<MapperTypeDto>()
                                    .map(<MapperTypeDto>::from)
                                    .map_err(|e| WriteError::ParseValue(Box::new(e)))?,
                            );
                        }
                        WritableKey::MapperTyC8y(key0, sub_key) => {
                            let mapper = self.mapper.try_get_mut(key0.as_deref(), "mapper")?;
                            let mapper_ty = mapper.ty.get_or_insert_with(|| MapperTypeDto::C8y { c8y: C8yDto::default() });
                            let mapper_ty_reader = reader
                                .mapper
                                .try_get(key0.as_deref())?
                                .ty
                                .or_none()
                                .map(::std::borrow::Cow::Borrowed)
                                .unwrap_or_else(|| {
                                    ::std::borrow::Cow::Owned(MapperTypeReader::C8y {
                                        c8y: C8yReader::from_dto(&C8yDto::default(), &TEdgeConfigLocation::default()),
                                    })
                                });
                            if let MapperTypeDto::C8y { c8y } = mapper_ty {
                                if let MapperTypeReader::C8y { c8y: c8y_reader } = mapper_ty_reader.as_ref() {
                                    c8y.try_append_str(c8y_reader, sub_key, value)?;
                                } else {
                                    unreachable!("Shape of reader should match shape of DTO")
                                }
                            } else {
                                return Err(WriteError::SuperFieldWrongValue {
                                    target: key.clone(),
                                    parent: WritableKey::MapperTy(key0.clone()),
                                    parent_expected: "c8y".to_string(),
                                    parent_actual: mapper_ty.to_string(),
                                });
                            }
                        }
                    };
                    Ok(())
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#actual)),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn try_remove_str_handles_sub_field_keys() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y), Custom])]
                #[tedge_config(rename = "type")]
                ty: MapperType,
            },
        );

        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let writers = generate_string_writers(&paths, &gen_ctx());
        let impl_block: syn::ItemImpl = syn::parse2(writers).unwrap();
        let actual = retain_fn(impl_block, "try_remove_str");

        let expected = parse_quote! {
            impl TEdgeConfigDto {
                pub fn try_remove_str(&mut self, reader: &TEdgeConfigReader, key: &WritableKey, value: &str) -> Result<(), WriteError> {
                    match key {
                        #[allow(clippy::useless_conversion)]
                        WritableKey::MapperType(key0) => {
                            self.mapper.try_get_mut(key0.as_deref(), "mapper")?.ty = <MapperTypeDto as AppendRemoveItem>::remove(
                                self.mapper.try_get_mut(key0.as_deref(), "mapper")?.ty.take(),
                                value
                                    .parse::<MapperTypeDto>()
                                    .map(<MapperTypeDto>::from)
                                    .map_err(|e| WriteError::ParseValue(Box::new(e)))?,
                            );
                        }
                        WritableKey::MapperTypeC8y(key0, sub_key) => {
                            let mapper = self.mapper.try_get_mut(key0.as_deref(), "mapper")?;
                            let mapper_ty = mapper.ty.get_or_insert_with(|| MapperTypeDto::C8y { c8y: C8yDto::default() });
                            let mapper_ty_reader = reader
                                .mapper
                                .try_get(key0.as_deref())?
                                .ty
                                .or_none()
                                .map(::std::borrow::Cow::Borrowed)
                                .unwrap_or_else(|| {
                                    ::std::borrow::Cow::Owned(MapperTypeReader::C8y {
                                        c8y: C8yReader::from_dto(&C8yDto::default(), &TEdgeConfigLocation::default()),
                                    })
                                });
                            if let MapperTypeDto::C8y { c8y } = mapper_ty {
                                if let MapperTypeReader::C8y { c8y: c8y_reader } = mapper_ty_reader.as_ref() {
                                    c8y.try_remove_str(c8y_reader, sub_key, value)?;
                                } else {
                                    unreachable!("Shape of reader should match shape of DTO")
                                }
                            } else {
                                return Err(WriteError::SuperFieldWrongValue {
                                    target: key.clone(),
                                    parent: WritableKey::MapperType(key0.clone()),
                                    parent_expected: "c8y".to_string(),
                                    parent_actual: mapper_ty.to_string(),
                                });
                            }
                        }
                    };
                    Ok(())
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#actual)),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn sub_field_keys_are_excluded_from_values_array() {
        // Regression test: ensure sub-field keys don't cause "Self is only available in impls" error
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y), Custom])]
                ty: MapperType,
            },
        );
        let gen_ctx = gen_ctx();

        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let config_keys =
            configuration_strings(paths.iter(), FilterRule::None, &gen_ctx.readable_key_name);
        let generated = keys_enum(&parse_quote!(ReadableKey), &config_keys, "read from");

        // Should parse successfully without "Self is only available" errors
        let generated_file: syn::File = syn::parse2(generated).unwrap();

        // Find the VALUES constant
        let impl_block = generated_file
            .items
            .iter()
            .find_map(|item| {
                if let syn::Item::Impl(r#impl @ syn::ItemImpl { trait_: None, .. }) = item {
                    Some(r#impl)
                } else {
                    None
                }
            })
            .expect("Should have impl block");

        let values_const = impl_block
            .items
            .iter()
            .find_map(|item| {
                if let syn::ImplItem::Const(c) = item {
                    if c.ident == "VALUES" {
                        return Some(c);
                    }
                }
                None
            })
            .expect("Should have VALUES const");

        // The VALUES array should only contain MapperType(None), not the sub-field variants
        let values_str = quote!(#values_const).to_string();
        assert!(
            values_str.contains("MapperTy"),
            "Should contain MapperTy (rename applied), got: {}",
            values_str
        );
        assert!(
            !values_str.contains("MapperTyC8y"),
            "Should not contain sub-field key MapperTyC8y"
        );
        assert!(
            !values_str.contains("unreachable"),
            "Should not contain unreachable! macro calls"
        );
    }

    #[test]
    fn sub_field_keys_are_excluded_from_fromstr() {
        // Regression test: ensure sub-field keys don't cause "Self is only available" error in FromStr
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y), Custom])]
                ty: MapperType,
            },
        );
        let gen_ctx = gen_ctx();

        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let config_keys =
            configuration_strings(paths.iter(), FilterRule::None, &gen_ctx.readable_key_name);
        let fromstr_impl =
            generate_fromstr_readable(&gen_ctx.readable_key_name.clone(), &config_keys, &gen_ctx);

        // Should parse successfully without "Self is only available" errors
        let _: syn::File =
            syn::parse2(fromstr_impl).expect("FromStr impl should parse without errors");
    }

    #[test]
    fn fromstr_parses_sub_field_keys() {
        // Test that FromStr properly parses sub-field keys
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y)])]
                #[tedge_config(rename = "type")]
                ty: MapperType,
            },
        );
        let gen_ctx = gen_ctx();

        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let config_keys =
            configuration_strings(paths.iter(), FilterRule::None, &gen_ctx.readable_key_name);
        let generated = generate_fromstr(
            &gen_ctx.readable_key_name,
            &config_keys,
            parse_quote!(_ => unimplemented!("just a test, no error handling")),
            &gen_ctx,
        );

        let expected = parse_quote!(
            impl ::std::str::FromStr for ReadableKey {
                type Err = ParseKeyError;
                fn from_str(value: &str) -> Result<Self, Self::Err> {
                    #[deny(unreachable_patterns)]
                    let res = match replace_aliases(value.to_owned()).replace(".", "_").as_str() {
                        "mapper_type" => {
                            if value != "mapper.type" {
                                warn_about_deprecated_key(value.to_owned(), "mapper.type");
                            }
                            return Ok(Self::MapperType(None));
                        },
                        key if key.starts_with("mapper_c8y_") => {
                            // Sub-field keys start with the prefix and parse the remainder with the sub-key type
                            let sub_key_str = value.strip_prefix("mapper.c8y.").unwrap_or(value);
                            let sub_key: C8yReadableKey = sub_key_str.parse().map_err(|err| match err {
                                C8yParseKeyError::ReadOnly(sub_key) => ParseKeyError::ReadOnly(ReadOnlyKey::MapperTypeC8y(None, sub_key)),
                                C8yParseKeyError::Unrecognised(sub_key) => ParseKeyError::Unrecognised(format!("mapper.c8y.{sub_key}")),
                            })?;
                            return Ok(Self::MapperTypeC8y(None, sub_key));
                        },
                        _ => unimplemented!("just a test, no error handling"),
                    };
                    if let Some(captures) = ::regex::Regex::new(#MAPPER_TY_REGEX).unwrap().captures(value) {
                        let key0 = captures.get(1usize).map(|re_match| re_match.as_str().to_owned());
                        return Ok(Self::MapperType(key0));
                    };
                    if let Some(captures) = ::regex::Regex::new(#MAPPER_TY_C8Y_REGEX).unwrap().captures(value) {
                        let key0 = captures.get(1usize).map(|re_match| re_match.as_str().to_owned());
                        let sub_key_str = captures.get(2usize).map(|re_match| re_match.as_str()).unwrap_or("");
                        let sub_key: C8yReadableKey = sub_key_str.parse().map_err({
                            let key0 = key0.clone();
                            |err| match err {
                                C8yParseKeyError::ReadOnly(sub_key) => ParseKeyError::ReadOnly(ReadOnlyKey::MapperTypeC8y(key0, sub_key)),
                                C8yParseKeyError::Unrecognised(sub_key) => ParseKeyError::Unrecognised(format!("mapper.c8y.{sub_key}")),
                            }
                        })?;
                        return Ok(Self::MapperTypeC8y(key0, sub_key));
                    };
                    res
                }
            }
        );

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&syn::parse2(generated).unwrap()),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn fromstr_parses_sub_field_keys_with_intermediary_group() {
        // Test that sub-field keys with an intermediary group still use correct capture indices
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                config: {
                    #[tedge_config(sub_fields = [C8y(C8y)])]
                    ty: MapperType,
                }
            },
        );
        let gen_ctx = &gen_ctx();

        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let config_keys =
            configuration_strings(paths.iter(), FilterRule::None, &gen_ctx.readable_key_name);
        let generated = generate_fromstr(
            &parse_quote!(ReadableKey),
            &config_keys,
            parse_quote!(_ => unimplemented!("just a test, no error handling")),
            gen_ctx,
        );

        let generated_code = prettyplease::unparse(&syn::parse2(generated).unwrap());

        // Find all capture.get(...) calls to see what indices are being used
        // Note: The pattern must account for potential whitespace/newlines between 'captures' and '.get'
        let capture_pattern =
            regex::Regex::new(r"captures\s*\.get\s*\(\s*(\d+)\s*usize\s*\)").unwrap();
        let capture_indices: Vec<usize> = capture_pattern
            .captures_iter(&generated_code)
            .filter_map(|cap| cap.get(1).and_then(|m| m.as_str().parse().ok()))
            .collect();

        // We should have exactly three capture groups: 1 for profile on mapper.config.ty, 1 for profile on mapper.config.ty.c8y.*, 2 for sub-key remainder
        if capture_indices != vec![1, 1, 2] {
            eprintln!("Expected [1, 1, 2] but found {:?}", capture_indices);
            eprintln!("Generated code:\n{}", generated_code);
            panic!(
                "Capture indices mismatch: expected [1, 1, 2], got {:?}",
                capture_indices
            );
        }
    }

    #[test]
    fn sub_field_variants_in_writable_key_use_writable_subkey_type() {
        // Test that WritableKey sub-field enum variants use WritableKey sub-field types,
        // not ReadableKey sub-field types
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                config: {
                    #[tedge_config(sub_fields = [C8y(C8y)])]
                    ty: MapperType,
                }
            },
        );

        let generated = generate_writable_keys(&ctx(), &input.groups);
        let generated_code = prettyplease::unparse(&syn::parse2(generated).unwrap());

        // Extract the WritableKey enum specifically
        let start_idx = generated_code
            .find("pub enum WritableKey")
            .expect("WritableKey enum not found");
        let end_idx = generated_code[start_idx..]
            .find("}\n")
            .expect("End of WritableKey enum not found")
            + start_idx;
        let writable_key_enum = &generated_code[start_idx..end_idx];

        // Should contain C8yWritableKey, not C8yReadableKey
        assert!(
            writable_key_enum.contains("C8yWritableKey"),
            "WritableKey should use C8yWritableKey. Found:\n{}",
            writable_key_enum
        );

        assert!(
            !writable_key_enum.contains("C8yReadableKey"),
            "WritableKey should NOT use C8yReadableKey. Found:\n{}",
            writable_key_enum
        );
    }

    #[test]
    fn sub_field_append_remove_uses_dto_type_not_reader_type() {
        // Regression test: ensure try_append_str and try_remove_str use the Dto type
        // for current_value when the field has sub-fields, not the Reader type.
        // Also tests that sub-field arms properly delegate to the sub-field DTO methods.
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y), Custom])]
                ty: MapperType,
            },
        );

        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let writers = generate_string_writers(&paths, &gen_ctx());

        let generated_file: syn::File = syn::parse2(writers).unwrap();

        // Find try_append_str method
        let try_append_method = generated_file
            .items
            .iter()
            .find_map(|item| {
                if let syn::Item::Impl(impl_block) = item {
                    impl_block.items.iter().find_map(|impl_item| {
                        if let syn::ImplItem::Fn(method) = impl_item {
                            if method.sig.ident == "try_append_str" {
                                return Some(method.clone());
                            }
                        }
                        None
                    })
                } else {
                    None
                }
            })
            .expect("Should have try_append_str method");

        let expected: syn::File = parse_quote! {
            pub fn try_append_str(
                &mut self,
                reader: &TEdgeConfigReader,
                key: &WritableKey,
                value: &str,
            ) -> Result<(), WriteError> {
                match key {
                    #[allow(clippy::useless_conversion)]
                    WritableKey::MapperTy(key0) => {
                        self.mapper.try_get_mut(key0.as_deref(), "mapper")?.ty = <MapperTypeDto as AppendRemoveItem>::append(
                            self.mapper.try_get_mut(key0.as_deref(), "mapper")?.ty.take(),
                            value
                                .parse::<MapperTypeDto>()
                                .map(<MapperTypeDto>::from)
                                .map_err(|e| WriteError::ParseValue(Box::new(e)))?,
                        );
                    }
                    WritableKey::MapperTyC8y(key0, sub_key) => {
                        let mapper = self.mapper.try_get_mut(key0.as_deref(), "mapper")?;
                        let mapper_ty = mapper.ty.get_or_insert_with(|| MapperTypeDto::C8y { c8y: C8yDto::default() });
                        let mapper_ty_reader = reader
                            .mapper
                            .try_get(key0.as_deref())?
                            .ty
                            .or_none()
                            .map(::std::borrow::Cow::Borrowed)
                            .unwrap_or_else(|| {
                                ::std::borrow::Cow::Owned(MapperTypeReader::C8y {
                                    c8y: C8yReader::from_dto(&C8yDto::default(), &TEdgeConfigLocation::default()),
                                })
                            });
                        if let MapperTypeDto::C8y { c8y } = mapper_ty {
                            if let MapperTypeReader::C8y { c8y: c8y_reader } = mapper_ty_reader.as_ref() {
                                c8y.try_append_str(c8y_reader, sub_key, value)?;
                            } else {
                                unreachable!("Shape of reader should match shape of DTO")
                            }
                        } else {
                            return Err(WriteError::SuperFieldWrongValue {
                                target: key.clone(),
                                parent: WritableKey::MapperTy(key0.clone()),
                                parent_expected: "c8y".to_string(),
                                parent_actual: mapper_ty.to_string(),
                            });
                        }
                    }
                };
                Ok(())
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#try_append_method)),
            prettyplease::unparse(&expected)
        );
    }

    #[test]
    fn to_cow_str_handles_sub_field_keys() {
        // Test that to_cow_str generates match arms for sub-field keys
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            mapper: {
                #[tedge_config(sub_fields = [C8y(C8y), Custom])]
                ty: MapperType,
            },
        );
        let gen_ctx = gen_ctx();

        let paths = configuration_paths_from(&input.groups, Mode::Reader);
        let config_keys =
            configuration_strings(paths.iter(), FilterRule::None, &gen_ctx.readable_key_name);
        let impl_block = keys_enum_impl_block(&config_keys);
        let actual = retain_fn(impl_block, "to_cow_str");

        let expected = parse_quote! {
            impl ReadableKey {
                pub fn to_cow_str(&self) -> ::std::borrow::Cow<'static, str> {
                    match self {
                        Self::MapperTy(None) => ::std::borrow::Cow::Borrowed("mapper.ty"),
                        Self::MapperTy(Some(key0)) => {
                            ::std::borrow::Cow::Owned(format!("mapper.profiles.{key0}.ty"))
                        }
                        Self::MapperTyC8y(key0, sub_key) => {
                            ::std::borrow::Cow::Owned(format!("{}.{}.{}", {
                                vec![if let Some(profile) = key0 {
                                    format!("mapper.profiles.{}", profile)
                                } else {
                                    "mapper".to_string()
                                }].join(".")
                            }, "c8y", sub_key.to_cow_str()))
                        }
                    }
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#actual)),
            prettyplease::unparse(&expected)
        );
    }

    fn keys_enum_impl_block(config_keys: &(Vec<String>, Vec<ConfigurationKey>)) -> ItemImpl {
        let generated = keys_enum(&parse_quote!(ReadableKey), config_keys, "DOC FRAGMENT");
        let generated_file: syn::File = syn::parse2(generated).unwrap();
        let mut impl_block = generated_file
            .items
            .into_iter()
            .find_map(|item| {
                if let syn::Item::Impl(r#impl @ ItemImpl { trait_: None, .. }) = item {
                    Some(r#impl)
                } else {
                    None
                }
            })
            .expect("Should generate an impl block for ReadableKey");

        // Remove doc comments from items
        for item in &mut impl_block.items {
            if let syn::ImplItem::Fn(f) = item {
                f.attrs.retain(|f| *f.path().get_ident().unwrap() != "doc");
            }
        }

        impl_block
    }

    fn retain_fn(mut impl_block: ItemImpl, fn_name: &str) -> ItemImpl {
        let ident = syn::Ident::new(fn_name, Span::call_site());
        let all_fn_names: Vec<_> = impl_block
            .items
            .iter()
            .filter_map(|i| match i {
                ImplItem::Fn(f) => Some(f.sig.ident.clone()),
                _ => None,
            })
            .collect();
        impl_block
            .items
            .retain(|i| matches!(i, ImplItem::Fn(f) if f.sig.ident == ident));
        assert!(
            !impl_block.items.is_empty(),
            "{ident:?} did not appear in methods. The valid method names are {all_fn_names:?}"
        );
        impl_block
    }

    fn is_doc_comment(attr: &syn::Attribute) -> bool {
        match &attr.meta {
            syn::Meta::NameValue(nv) => {
                nv.path.get_ident().map(<_>::to_string) == Some("doc".into())
            }
            _ => false,
        }
    }

    fn ctx() -> CodegenContext {
        CodegenContext::default_tedge_config()
    }

    fn gen_ctx() -> GenerationContext {
        GenerationContext::from(&ctx())
    }
}
