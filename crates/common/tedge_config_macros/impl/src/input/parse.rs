//! The initial parsing logic
//!
//! This is designed to take a [proc_macro2::TokenStream] and turn it into
//! something useful with the aid of [syn].
use darling::util::SpannedValue;
use darling::FromAttributes;
use darling::FromField;
use darling::FromMeta;
use syn::parse::Parse;
use syn::punctuated::Punctuated;
use syn::Attribute;
use syn::Token;

#[derive(Debug)]
pub struct Configuration {
    pub groups: Punctuated<FieldOrGroup, Token![,]>,
}

impl Parse for Configuration {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            groups: input.parse_terminated(<_>::parse, Token![,])?,
        })
    }
}

#[derive(FromAttributes)]
#[darling(attributes(tedge_config))]
pub struct ConfigurationAttributes {
    #[darling(default)]
    pub dto: GroupDtoSettings,
    #[darling(default)]
    pub reader: ReaderSettings,
    #[darling(default, multiple, rename = "deprecated_name")]
    pub deprecated_names: Vec<SpannedValue<String>>,
    #[darling(default)]
    pub rename: Option<SpannedValue<String>>,
}

#[derive(Debug)]
pub struct ConfigurationGroup {
    pub attrs: Vec<syn::Attribute>,
    pub dto: GroupDtoSettings,
    pub reader: ReaderSettings,
    pub deprecated_names: Vec<SpannedValue<String>>,
    pub rename: Option<SpannedValue<String>>,
    pub ident: syn::Ident,
    pub colon_token: Token![:],
    pub brace: syn::token::Brace,
    pub content: Punctuated<FieldOrGroup, Token![,]>,
}

impl Parse for ConfigurationGroup {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let content;
        let attributes = input.call(Attribute::parse_outer)?;
        let known_attributes = ConfigurationAttributes::from_attributes(&attributes)?;
        Ok(ConfigurationGroup {
            attrs: attributes.into_iter().filter(not_tedge_config).collect(),
            dto: known_attributes.dto,
            reader: known_attributes.reader,
            deprecated_names: known_attributes.deprecated_names,
            rename: known_attributes.rename,
            ident: input.parse()?,
            colon_token: input.parse()?,
            brace: syn::braced!(content in input),
            content: content.parse_terminated(<_>::parse, Token![,])?,
        })
    }
}

fn not_tedge_config(attr: &syn::Attribute) -> bool {
    let is_tedge_config = match &attr.meta {
        syn::Meta::List(list) => list.path.is_ident("tedge_config"),
        _ => false,
    };

    !is_tedge_config
}

#[derive(Debug)]
pub enum FieldOrGroup {
    Field(ConfigurableField),
    Group(ConfigurationGroup),
}

impl Parse for FieldOrGroup {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let fork = input.fork();

        fork.call(Attribute::parse_outer)?;
        fork.parse::<syn::Ident>()?;
        fork.parse::<Token![:]>()?;

        let lookahead = fork.lookahead1();
        if lookahead.peek(syn::token::Brace) {
            input.parse().map(Self::Group)
        } else {
            input.parse().map(Self::Field)
        }
    }
}

#[derive(FromField, Debug)]
#[darling(attributes(tedge_config), forward_attrs)]
pub struct ConfigurableField {
    pub attrs: Vec<syn::Attribute>,
    #[darling(default)]
    pub readonly: Option<ReadonlySettings>,
    #[darling(default)]
    pub dto: FieldDtoSettings,
    #[darling(default)]
    pub rename: Option<SpannedValue<String>>,
    #[darling(multiple, rename = "deprecated_key")]
    pub deprecated_keys: Vec<SpannedValue<String>>,
    #[darling(multiple, rename = "deprecated_name")]
    pub deprecated_names: Vec<SpannedValue<String>>,
    #[darling(default)]
    // TODO remove this or separate it from the group ones
    pub reader: ReaderSettings,
    #[darling(default)]
    pub default: Option<FieldDefault>,
    #[darling(default)]
    pub note: Option<SpannedValue<String>>,
    #[darling(multiple, rename = "example")]
    pub examples: Vec<SpannedValue<String>>,
    pub ident: Option<syn::Ident>,
    pub ty: syn::Type,
}

#[derive(Debug, FromMeta, PartialEq, Eq)]
pub enum FieldDefault {
    Variable(syn::Path),
    Function(syn::Expr),
    FromKey(Punctuated<syn::Ident, syn::Token![.]>),
    FromOptionalKey(Punctuated<syn::Ident, syn::Token![.]>),
    Value(syn::Lit),
    None,
}

impl Parse for ConfigurableField {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self::from_field(&input.call(syn::Field::parse_named)?)?)
    }
}

#[derive(FromMeta, Debug, Default)]
pub struct GroupDtoSettings {
    #[darling(default)]
    pub skip: bool,
    #[darling(default)]
    pub flatten: bool,
}

#[derive(FromMeta, Debug, Default)]
pub struct FieldDtoSettings {
    #[darling(default)]
    pub skip: bool,
}

#[derive(FromMeta, Debug, Default)]
pub struct ReaderSettings {
    #[darling(default)]
    pub private: bool,
    #[darling(default)]
    pub skip: bool,
}

#[derive(FromMeta, Debug)]
pub struct ReadonlySettings {
    pub write_error: String,
    pub function: syn::Path,
}
