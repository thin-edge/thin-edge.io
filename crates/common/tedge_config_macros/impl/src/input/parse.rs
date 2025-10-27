//! The initial parsing logic
//!
//! This is designed to take a [proc_macro2::TokenStream] and turn it into
//! something useful with the aid of [syn].

use darling::util::SpannedValue;
use darling::FromAttributes;
use darling::FromField;
use darling::FromMeta;
use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::parse::Parse;
use syn::punctuated::Punctuated;
use syn::Attribute;
use syn::Expr;
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

#[derive(Debug)]
pub struct SubConfigInput {
    pub name: syn::Ident,
    pub config: Configuration,
}

impl Parse for SubConfigInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        let content;
        syn::braced!(content in input);
        let config = Configuration::parse(&content)?;
        Ok(Self { name, config })
    }
}

#[derive(FromAttributes)]
#[darling(attributes(tedge_config))]
pub struct ConfigurationAttributes {
    #[darling(default)]
    pub dto: GroupDtoSettings,
    #[darling(default)]
    pub multi: bool,
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
    pub multi: bool,
    pub deprecated_names: Vec<SpannedValue<String>>,
    pub rename: Option<SpannedValue<String>>,
    pub ident: syn::Ident,
    pub content: Punctuated<FieldOrGroup, Token![,]>,
}

impl Parse for ConfigurationGroup {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let content;
        let attributes = input.call(Attribute::parse_outer)?;
        let known_attributes = ConfigurationAttributes::from_attributes(&attributes)?;
        let ident = input.parse()?;
        input.parse::<Token![:]>()?;
        syn::braced!(content in input);
        let content = content.parse_terminated(<_>::parse, Token![,])?;
        Ok(ConfigurationGroup {
            attrs: attributes.into_iter().filter(not_tedge_config).collect(),
            dto: known_attributes.dto,
            reader: known_attributes.reader,
            deprecated_names: known_attributes.deprecated_names,
            rename: known_attributes.rename,
            multi: known_attributes.multi,
            ident,
            content,
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

#[allow(clippy::large_enum_variant)] // macro code, low impact
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
    #[darling(default)]
    pub sub_fields: Option<SpannedValue<EnumEntries>>,
    pub ident: Option<syn::Ident>,
    pub ty: syn::Type,
    #[darling(default)]
    pub from: Option<syn::Type>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EnumEntry {
    NameOnly(syn::Ident),
    NameAndFields(syn::Ident, syn::Ident),
}

impl EnumEntry {
    pub fn span(&self) -> proc_macro2::Span {
        match self {
            Self::NameOnly(name) | Self::NameAndFields(name, _) => name.span(),
        }
    }
}

impl Parse for EnumEntry {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let ident: syn::Ident = input.parse()?;

        if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            let ty: syn::Ident = content.parse()?;
            Ok(EnumEntry::NameAndFields(ident, ty))
        } else {
            Ok(EnumEntry::NameOnly(ident))
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct EnumEntries(pub Vec<EnumEntry>);

impl FromMeta for EnumEntries {
    fn from_expr(expr: &Expr) -> darling::Result<Self> {
        match expr {
            Expr::Array(array) => {
                let entries: syn::Result<Vec<_>> = array
                    .elems
                    .iter()
                    .map(|elem| syn::parse2(elem.to_token_stream()))
                    .collect();
                Ok(EnumEntries(entries?))
            }
            _ => Err(darling::Error::custom(
                "Expected an array of enum entries like [C8y(C8y), Custom]",
            )),
        }
    }
}

#[derive(Debug, FromMeta, PartialEq, Eq)]
pub enum FieldDefault {
    Variable(syn::Path),
    Function(syn::Expr),
    FromKey(Punctuated<syn::Ident, syn::Token![.]>),
    FromOptionalKey(Punctuated<syn::Ident, syn::Token![.]>),
    Value(DefaultValueLit),
    FromStr(syn::LitStr),
    None,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DefaultValueLit(syn::Lit);

impl ToTokens for DefaultValueLit {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens)
    }
}

impl FromMeta for DefaultValueLit {
    fn from_expr(expr: &Expr) -> darling::Result<Self> {
        match expr {
            Expr::Lit(value) => Ok(Self(value.lit.clone())),
            _ => Err(darling::Error::custom(format!(
                "Unexpected expression, `default(value = ...)` expects a literal.\n\
                 Perhaps you want to use `#[tedge_config(default(variable = \"{}\"))]`?",
                quote::quote!(#expr).to_string().replace(" :: ", "::")
            ))),
        }
    }
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
    pub function: Option<syn::Path>,
    #[darling(default)]
    pub skip: bool,
}

#[derive(FromMeta, Debug)]
pub struct ReadonlySettings {
    pub write_error: String,
    pub function: syn::Path,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enum_attribute_parsing() {
        let field: ConfigurableField = syn::parse_quote! {
            #[tedge_config(sub_fields = [C8y(C8y), Aws(Aws), Custom])]
            ty: BridgeType
        };

        let c8y_ident = syn::parse_quote!(C8y);
        let c8y_type = syn::parse_quote!(C8y);
        let aws_ident = syn::parse_quote!(Aws);
        let aws_type = syn::parse_quote!(Aws);
        let custom_ident = syn::parse_quote!(Custom);

        let expected = EnumEntries(vec![
            EnumEntry::NameAndFields(c8y_ident, c8y_type),
            EnumEntry::NameAndFields(aws_ident, aws_type),
            EnumEntry::NameOnly(custom_ident),
        ]);

        assert_eq!(&**field.sub_fields.as_ref().unwrap(), &expected);
        assert_eq!(field.ident.as_ref().unwrap().to_string(), "ty");
    }

    #[test]
    fn test_sub_config_input_parsing() {
        let input: SubConfigInput = syn::parse_quote! {
            BridgeConfig {
                bridge_azure: {
                    url: String,
                },
                bridge_aws: {
                    region: String,
                },
            }
        };

        assert_eq!(input.name.to_string(), "BridgeConfig");
        assert_eq!(input.config.groups.len(), 2);
    }
}
