use crate::error::extract_type_from_result;
use crate::input::ConfigurableField;
use crate::input::FieldOrGroup;
use crate::namegen::IdGenerator;
use crate::namegen::SequentialIdGenerator;
use crate::namegen::UnderscoreIdGenerator;
use heck::ToSnekCase;
use heck::ToUpperCamelCase;
use itertools::Itertools;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;
use std::borrow::Cow;
use std::collections::VecDeque;
use syn::parse_quote;
use syn::parse_quote_spanned;
use syn::spanned::Spanned;

pub fn generate_writable_keys(items: &[FieldOrGroup]) -> TokenStream {
    let mut paths = configuration_paths_from(items);
    let (readonly_destr, write_error): (Vec<_>, Vec<_>) = paths
        .iter()
        .filter_map(|field| {
            let configuration = enum_variant(field);
            Some((
                configuration.match_shape,
                field
                    .back()?
                    .field()?
                    .read_only()?
                    .readonly
                    .write_error
                    .as_str(),
            ))
        })
        .multiunzip();
    let readable_args = configuration_strings(paths.iter());
    let readonly_args = configuration_strings(paths.iter().filter(|path| !is_read_write(path)));
    let writable_args = configuration_strings(paths.iter().filter(|path| is_read_write(path)));
    let readable_keys = keys_enum(parse_quote!(ReadableKey), &readable_args, "read from");
    let readonly_keys = keys_enum(
        parse_quote!(ReadOnlyKey),
        &readonly_args,
        "read from, but not written to,",
    );
    let writable_keys = keys_enum(parse_quote!(WritableKey), &writable_args, "written to");
    let fromstr_readable = generate_fromstr_readable(parse_quote!(ReadableKey), &readable_args);
    let fromstr_readonly = generate_fromstr_readable(parse_quote!(ReadOnlyKey), &readonly_args);
    let fromstr_writable = generate_fromstr_writable(parse_quote!(WritableKey), &writable_args);
    let read_string = generate_string_readers(&paths);
    let write_string = generate_string_writers(
        &paths
            .iter()
            .filter(|path| is_read_write(path))
            .cloned()
            .collect::<Vec<_>>(),
    );

    let paths_vec = paths
        .iter_mut()
        .map(|vd| &*vd.make_contiguous())
        .collect::<Vec<_>>();
    let readable_keys_iter = key_iterators(
        parse_quote!(TEdgeConfigReader),
        parse_quote!(ReadableKey),
        &paths_vec,
        "",
        &[],
    );
    let readonly_keys_iter = key_iterators(
        parse_quote!(TEdgeConfigReader),
        parse_quote!(ReadOnlyKey),
        &paths_vec
            .iter()
            .copied()
            .filter(|r| r.last().unwrap().field().unwrap().read_only().is_some())
            .collect::<Vec<_>>(),
        "",
        &[],
    );
    let writable_keys_iter = key_iterators(
        parse_quote!(TEdgeConfigReader),
        parse_quote!(WritableKey),
        &paths_vec
            .iter()
            .copied()
            .filter(|r| r.last().unwrap().field().unwrap().read_only().is_none())
            .collect::<Vec<_>>(),
        "",
        &[],
    );

    let (static_alias, deprecated_keys) = deprecated_keys(paths.iter());
    let iter_updated = deprecated_keys.iter().map(|k| &k.iter_field);

    let fallback_branch: Option<syn::Arm> = readonly_args
        .0
        .is_empty()
        .then(|| parse_quote!(_ => unreachable!("ReadOnlyKey is uninhabited")));

    quote! {
        #readable_keys
        #readonly_keys
        #writable_keys
        #fromstr_readable
        #fromstr_readonly
        #fromstr_writable
        #read_string
        #write_string
        #readable_keys_iter
        #readonly_keys_iter
        #writable_keys_iter

        #[derive(::thiserror::Error, Debug)]
        /// An error encountered when writing to a configuration value from a
        /// string
        pub enum WriteError {
            #[error("Failed to parse input")]
            ParseValue(#[from] Box<dyn ::std::error::Error + Send + Sync>),
            #[error(transparent)]
            Multi(#[from] MultiError),
        }

        impl ReadOnlyKey {
            fn write_error(&self) -> &'static str {
                match self {
                    #(Self::#readonly_destr => #write_error,)*
                    #fallback_branch
                }
            }
        }

        #[derive(Debug, ::thiserror::Error)]
        /// An error encountered when parsing a configuration key from a string
        pub enum ParseKeyError {
            #[error("{}", .0.write_error())]
            ReadOnly(ReadOnlyKey),
            #[error("Unknown key: '{0}'")]
            Unrecognised(String),
        }

        fn replace_aliases(key: String) -> String {
            use ::once_cell::sync::Lazy;
            use ::std::borrow::Cow;
            use ::std::collections::HashMap;
            use ::doku::*;

            static ALIASES: Lazy<HashMap<Cow<'static, str>, Cow<'static, str>>> = Lazy::new(|| {
                let ty = TEdgeConfigReader::ty();
                let TypeKind::Struct { fields, transparent: false } = ty.kind else { panic!("Expected struct but got {:?}", ty.kind) };
                let Fields::Named { fields } = fields else { panic!("Expected named fields but got {:?}", fields)};
                let mut aliases = struct_field_aliases(None, &fields);
                #(
                    if let Some(alias) = aliases.insert(Cow::Borrowed(#static_alias), ReadableKey::#iter_updated.to_cow_str()) {
                        panic!("Duplicate configuration alias for '{}'. It maps to both '{}' and '{}'. Perhaps you provided an incorrect `deprecated_key` for one of these configurations?", #static_alias, alias, ReadableKey::#iter_updated.to_cow_str());
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
}

fn configuration_strings<'a>(
    variants: impl Iterator<Item = &'a VecDeque<&'a FieldOrGroup>>,
) -> (Vec<String>, Vec<ConfigurationKey>) {
    variants
        .map(|segments| {
            let configuration_key = enum_variant(segments);
            (
                segments
                    .iter()
                    .map(|variant| variant.name())
                    .collect::<Vec<_>>()
                    .join("."),
                configuration_key,
            )
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
    type_name: syn::Ident,
    (configuration_string, configuration_key): &(Vec<String>, Vec<ConfigurationKey>),
    error_case: syn::Arm,
) -> TokenStream {
    let simplified_configuration_string = configuration_string
        .iter()
        .map(|s| s.replace('.', "_"))
        .zip(configuration_key.iter().map(|k| &k.enum_variant))
        .map(|(s, v)| quote_spanned!(v.span()=> #s));
    let iter_variant = configuration_key.iter().map(|k| &k.iter_field);
    let regex_patterns =
        configuration_key
            .iter()
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

    quote! {
        impl ::std::str::FromStr for #type_name {
            type Err = ParseKeyError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                // If we get an unreachable pattern, it means we have the same key twice
                #[deny(unreachable_patterns)]
                let res = match replace_aliases(value.to_owned()).replace(".", "_").as_str() {
                    #(
                        #simplified_configuration_string => {
                            if value != #configuration_string {
                                warn_about_deprecated_key(value.to_owned(), #configuration_string);
                            }
                            return Ok(Self::#iter_variant)
                        },
                    )*
                    #error_case
                };
                #(#regex_patterns;)*
                res
            }
        }
    }
}

fn generate_fromstr_readable(
    type_name: syn::Ident,
    fields: &(Vec<String>, Vec<ConfigurationKey>),
) -> TokenStream {
    generate_fromstr(
        type_name,
        fields,
        parse_quote! { _ => Err(ParseKeyError::Unrecognised(value.to_owned())) },
    )
}

// TODO test the error messages actually appear
fn generate_fromstr_writable(
    type_name: syn::Ident,
    fields: &(Vec<String>, Vec<ConfigurationKey>),
) -> TokenStream {
    generate_fromstr(
        type_name,
        fields,
        parse_quote! {
            _ => if let Ok(key) = <ReadOnlyKey as ::std::str::FromStr>::from_str(value) {
                Err(ParseKeyError::ReadOnly(key))
            } else {
                Err(ParseKeyError::Unrecognised(value.to_owned()))
            },
        },
    )
}

fn key_iterators(
    reader_ty: syn::Ident,
    type_name: syn::Ident,
    fields: &[&[&FieldOrGroup]],
    prefix: &str,
    args: &[syn::Ident],
) -> TokenStream {
    let mut function_name = type_name.to_string().to_snek_case();
    // Pluralise the name
    function_name += "s";
    let function_name = syn::Ident::new(&function_name, type_name.span());

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
                    sub_type_name,
                    type_name.clone(),
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
                    sub_type_name,
                    type_name.clone(),
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
                let field_name = syn::Ident::new(
                    &format!(
                        "{}{}",
                        prefix,
                        f.rename()
                            .map(<_>::to_upper_camel_case)
                            .unwrap_or_else(|| ident.to_string().to_upper_camel_case())
                    ),
                    ident.span(),
                );
                let args = match args.len() {
                    0 => TokenStream::new(),
                    _ => {
                        quote!((#(#args.clone()),*))
                    }
                };
                complete_fields.push(parse_quote!(#type_name::#field_name #args))
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
            parse_quote!(chain(#expr))
        } else {
            expr
        }
    });

    quote! {
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
    type_name: syn::Ident,
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
    let uninhabited_catch_all = configuration_key
        .is_empty()
        .then_some::<syn::Arm>(parse_quote!(_ => unimplemented!("Cope with empty enum")));

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
                let field = id_gen.next_id(ident.span());
                quote_spanned!(ident.span()=> #ident.#method(#field.as_deref())?)
            }
            FieldOrGroup::Multi(_) => {
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

fn generate_string_readers(paths: &[VecDeque<&FieldOrGroup>]) -> TokenStream {
    let enum_variants = paths.iter().map(enum_variant);
    let arms = paths
        .iter()
        .zip(enum_variants)
        .map(|(path, configuration_key)| -> syn::Arm {
            let field = path
                .back()
                .expect("Path must have a back as it is nonempty")
                .field()
                .expect("Back of path is guaranteed to be a field");
            let segments = generate_field_accessor(path, "try_get", true);
            let to_string = quote_spanned!(field.ty().span()=> .to_string());
            let match_variant = configuration_key.match_read_write;
            if field.read_only().is_some() {
                if extract_type_from_result(field.ty()).is_some() {
                    parse_quote! {
                        ReadableKey::#match_variant => Ok(self.#(#segments).*.try_read(self)?#to_string),
                    }
                } else {
                    parse_quote! {
                        ReadableKey::#match_variant => Ok(self.#(#segments).*.read(self)#to_string),
                    }
                }
            } else if field.has_guaranteed_default() {
                parse_quote! {
                    ReadableKey::#match_variant => Ok(self.#(#segments).*#to_string),
                }
            } else {
                parse_quote! {
                    ReadableKey::#match_variant => Ok(self.#(#segments).*.or_config_not_set()?#to_string),
                }
            }
        });
    let fallback_branch: Option<syn::Arm> = paths
        .is_empty()
        .then(|| parse_quote!(_ => unreachable!("ReadableKey is uninhabited")));
    quote! {
        impl TEdgeConfigReader {
            pub fn read_string(&self, key: &ReadableKey) -> Result<String, ReadError> {
                match key {
                    #(#arms)*
                    #fallback_branch
                }
            }
        }
    }
}

fn generate_string_writers(paths: &[VecDeque<&FieldOrGroup>]) -> TokenStream {
    let enum_variants = paths.iter().map(enum_variant);
    let (update_arms, unset_arms, append_arms, remove_arms): (
        Vec<syn::Arm>,
        Vec<syn::Arm>,
        Vec<syn::Arm>,
        Vec<syn::Arm>,
    ) = paths
        .iter()
        .zip(enum_variants)
        .map(|(path, configuration_key)| {
            let read_segments = generate_field_accessor(path, "try_get", true);
            let write_segments = generate_field_accessor(path, "try_get_mut", false).collect::<Vec<_>>();
            let field = path
                .iter()
                .filter_map(|thing| thing.field())
                .next()
                .unwrap();
            let match_variant = configuration_key.match_read_write;

            let ty = field.ty();
            let parse_as = field.from().unwrap_or(field.ty());
            let parse = quote_spanned! {parse_as.span()=> parse::<#parse_as>() };
            let convert_to_field_ty = quote_spanned! {ty.span()=> map(<#ty>::from)};

            let current_value = if field.read_only().is_some() {
                if extract_type_from_result(field.ty()).is_some() {
                    quote_spanned! {ty.span()=> reader.#(#read_segments).*.try_read(reader).ok()}
                } else {
                    quote_spanned! {ty.span()=> Some(reader.#(#read_segments).*.read(reader))}
                }
            } else if field.has_guaranteed_default() {
                quote_spanned! {ty.span()=> Some(reader.#(#read_segments).*.to_owned())}
            } else {
                quote_spanned! {ty.span()=> reader.#(#read_segments).*.or_none().cloned()}
            };

            (
                parse_quote_spanned! {ty.span()=>
                    WritableKey::#match_variant => self.#(#write_segments).* = Some(value
                        .#parse
                        .#convert_to_field_ty
                        .map_err(|e| WriteError::ParseValue(Box::new(e)))?),
                },
                parse_quote_spanned! {ty.span()=>
                    WritableKey::#match_variant => self.#(#write_segments).* = None,
                },
                parse_quote_spanned! {ty.span()=>
                    WritableKey::#match_variant => self.#(#write_segments).* = <#ty as AppendRemoveItem>::append(
                        #current_value,
                        value
                        .#parse
                        .#convert_to_field_ty
                        .map_err(|e| WriteError::ParseValue(Box::new(e)))?),
                },
                parse_quote_spanned! {ty.span()=>
                    WritableKey::#match_variant => self.#(#write_segments).* = <#ty as AppendRemoveItem>::remove(
                        #current_value,
                        value
                        .#parse
                        .#convert_to_field_ty
                        .map_err(|e| WriteError::ParseValue(Box::new(e)))?),
                },
            )
        })
        .multiunzip();
    let fallback_branch: Option<syn::Arm> = update_arms
        .is_empty()
        .then(|| parse_quote!(_ => unreachable!("WritableKey is uninhabited")));

    quote! {
        impl TEdgeConfigDto {
            pub fn try_update_str(&mut self, key: &WritableKey, value: &str) -> Result<(), WriteError> {
                match key {
                    #(#update_arms)*
                    #fallback_branch
                };
                Ok(())
            }

            pub fn try_unset_key(&mut self, key: &WritableKey) -> Result<(), WriteError> {
                match key {
                    #(#unset_arms)*
                    #fallback_branch
                };
                Ok(())
            }

            pub fn try_append_str(&mut self, reader: &TEdgeConfigReader, key: &WritableKey, value: &str) -> Result<(), WriteError> {
                match key {
                    #(#append_arms)*
                    #fallback_branch
                };
                Ok(())
            }

            pub fn try_remove_str(&mut self, reader: &TEdgeConfigReader, key: &WritableKey, value: &str) -> Result<(), WriteError> {
                match key {
                    #(#remove_arms)*
                    #fallback_branch
                };
                Ok(())
            }
        }
    }
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
        .interleave(std::iter::repeat(Cow::Borrowed(".")).take(segments.len() - 1))
        .collect::<String>();
    if count_multi > 0 {
        let opt_strs =
            std::iter::repeat::<syn::Type>(parse_quote!(Option<String>)).take(count_multi);
        let enum_variant = parse_quote_spanned!(ident.span()=> #ident(#(#opt_strs),*));
        let nones = std::iter::repeat::<syn::Path>(parse_quote!(None)).take(count_multi);
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
                FieldOrGroup::Multi(m) => format!("{}(?:\\.(@[A-z_]+))?", m.ident),
                FieldOrGroup::Field(f) => f.ident().to_string(),
                FieldOrGroup::Group(g) => g.ident.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\\.");
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
                                    format!("{}.{{{}}}", m.ident, binding)
                                }
                            }
                            FieldOrGroup::Group(g) => g.ident.to_string(),
                            FieldOrGroup::Field(f) => f.ident().to_string(),
                        })
                        .interleave(std::iter::repeat(".".to_owned()).take(segments.len() - 1))
                        .collect::<String>();
                    (
                        parse_quote!(#ident(#(#options),*)),
                        parse_quote!(::std::borrow::Cow::Owned(format!(#format_str))),
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
        }
    }
}

/// Generates a list of the toml paths for each of the keys in the provided
/// configuration
fn configuration_paths_from(items: &[FieldOrGroup]) -> Vec<VecDeque<&FieldOrGroup>> {
    let mut res = vec![];
    for item in items.iter().filter(|item| !item.reader().skip) {
        match item {
            FieldOrGroup::Field(_) => res.push(VecDeque::from([item])),
            FieldOrGroup::Group(group) | FieldOrGroup::Multi(group) => {
                for mut fields in configuration_paths_from(&group.contents) {
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
    use syn::ItemImpl;

    #[test]
    fn output_parses() {
        syn::parse2::<syn::File>(generate_writable_keys(&[])).unwrap();
    }

    #[test]
    fn output_parses_for_multi() {
        let input: crate::input::Configuration = parse_quote! {
            #[tedge_config(multi)]
            c8y: {
                url: String
            }
        };
        syn::parse2::<syn::File>(generate_writable_keys(&input.groups)).unwrap();
    }

    #[test]
    fn from_str_does_not_generate_regex_matches_for_normal_fields() {
        let input: crate::input::Configuration = parse_quote!(
            c8y: {
                url: String,
            }
        );
        let paths = configuration_paths_from(&input.groups);
        let c = configuration_strings(paths.iter());
        let generated = generate_fromstr(
            syn::Ident::new("ReadableKey", Span::call_site()),
            &c,
            parse_quote!(_ => unimplemented!("just a test, no error handling")),
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

    #[test]
    fn from_str_generates_regex_matches_for_multi_fields() {
        let input: crate::input::Configuration = parse_quote!(
            #[tedge_config(multi)]
            c8y: {
                url: String,
            }
        );
        let paths = configuration_paths_from(&input.groups);
        let c = configuration_strings(paths.iter());
        let generated = generate_fromstr(
            syn::Ident::new("ReadableKey", Span::call_site()),
            &c,
            parse_quote!(_ => unimplemented!("just a test, no error handling")),
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
                    if let Some(captures) = ::regex::Regex::new("^c8y(?:\\.(@[A-z_]+))?\\.url$").unwrap().captures(value) {
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
        let mut paths = configuration_paths_from(&input.groups);
        let paths = paths.iter_mut().map(|vd| &*vd.make_contiguous());
        let generated = key_iterators(
            parse_quote!(TEdgeConfigReader),
            parse_quote!(ReadableKey),
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
        let mut paths = configuration_paths_from(&input.groups);
        let paths = paths.iter_mut().map(|vd| &*vd.make_contiguous());
        let generated = key_iterators(
            parse_quote!(TEdgeConfigReader),
            parse_quote!(ReadableKey),
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
    fn iteration_of_empty_field_enum_is_an_empty_iterator() {
        let generated = key_iterators(
            parse_quote!(TEdgeConfigReader),
            parse_quote!(ReadableKey),
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
        let mut paths = configuration_paths_from(&input.groups);
        let paths = paths.iter_mut().map(|vd| &*vd.make_contiguous());
        let generated = key_iterators(
            parse_quote!(TEdgeConfigReader),
            parse_quote!(ReadableKey),
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
        let paths = configuration_paths_from(&input.groups);
        let config_keys = configuration_strings(paths.iter());
        let impl_block = keys_enum_impl_block(&config_keys);

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
            }
        );
        let paths = configuration_paths_from(&input.groups);
        let config_keys = configuration_strings(paths.iter());
        let impl_block = keys_enum_impl_block(&config_keys);

        let expected = parse_quote! {
            impl ReadableKey {
                pub fn to_cow_str(&self) -> ::std::borrow::Cow<'static, str> {
                    match self {
                        Self::C8yUrl(None) => ::std::borrow::Cow::Borrowed("c8y.url"),
                        Self::C8yUrl(Some(key0)) => ::std::borrow::Cow::Owned(format!("c8y.{key0}.url")),
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
        let paths = configuration_paths_from(&input.groups);
        let config_keys = configuration_strings(paths.iter());
        let impl_block = keys_enum_impl_block(&config_keys);

        let expected = parse_quote! {
            impl ReadableKey {
                pub fn to_cow_str(&self) -> ::std::borrow::Cow<'static, str> {
                    match self {
                        Self::TopNestedField(None, None) => ::std::borrow::Cow::Borrowed("top.nested.field"),
                        Self::TopNestedField(None, Some(key1)) => ::std::borrow::Cow::Owned(format!("top.nested.{key1}.field")),
                        Self::TopNestedField(Some(key0), None) => ::std::borrow::Cow::Owned(format!("top.{key0}.nested.field")),
                        Self::TopNestedField(Some(key0), Some(key1)) => ::std::borrow::Cow::Owned(format!("top.{key0}.nested.{key1}.field")),
                    }
                }
            }
        };

        pretty_assertions::assert_eq!(
            prettyplease::unparse(&parse_quote!(#impl_block)),
            prettyplease::unparse(&expected)
        );
    }

    fn keys_enum_impl_block(config_keys: &(Vec<String>, Vec<ConfigurationKey>)) -> ItemImpl {
        let generated = keys_enum(parse_quote!(ReadableKey), config_keys, "DOC FRAGMENT");
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
}
