use std::borrow::Cow;

use darling::export::NestedMeta;
use darling::util::SpannedValue;
use heck::ToUpperCamelCase;
use quote::format_ident;
use syn::parse_quote;
use syn::parse_quote_spanned;
use syn::spanned::Spanned;
use syn::Expr;
use syn::Lit;
use syn::Meta;
use syn::MetaNameValue;

use crate::error::combine_errors;
use crate::error::extract_type_from_result;
use crate::optional_error::OptionalError;
use crate::optional_error::SynResultExt;
use crate::reader::PathItem;

pub use super::parse::EnumEntry;
pub use super::parse::FieldDefault;
pub use super::parse::FieldDtoSettings;
pub use super::parse::GroupDtoSettings;
pub use super::parse::ReaderSettings;
use super::parse::ReadonlySettings;
pub use super::parse::SubConfigInput;

#[derive(Debug)]
pub struct Configuration {
    pub groups: Vec<FieldOrGroup>,
}

impl syn::parse::Parse for Configuration {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        super::parse::Configuration::parse(input)?.try_into()
    }
}

impl TryFrom<super::parse::Configuration> for Configuration {
    type Error = syn::Error;

    fn try_from(value: super::parse::Configuration) -> Result<Self, Self::Error> {
        Ok(Self {
            groups: combine_errors(value.groups.into_iter().map(<_>::try_from))?,
        })
    }
}

impl Configuration {
    /// Validate that multi-profile groups are not used in a sub-config
    pub fn validate_for_sub_config(&self) -> Result<(), syn::Error> {
        for group in &self.groups {
            validate_no_multi_in_sub_config(group)?;
        }
        Ok(())
    }
}

fn validate_no_multi_in_sub_config(field_or_group: &FieldOrGroup) -> Result<(), syn::Error> {
    match field_or_group {
        FieldOrGroup::Multi(group) => Err(syn::Error::new(
            group.ident.span(),
            "Multi-profile groups are not supported in `define_sub_config!`",
        )),
        FieldOrGroup::Group(group) => {
            for content in &group.contents {
                validate_no_multi_in_sub_config(content)?;
            }
            Ok(())
        }
        FieldOrGroup::Field(_) => Ok(()),
    }
}

#[derive(Debug)]
pub struct ConfigurationGroup {
    pub attrs: Vec<syn::Attribute>,
    pub rename: Option<SpannedValue<String>>,
    pub dto: GroupDtoSettings,
    pub reader: ReaderSettings,
    pub ident: syn::Ident,
    pub contents: Vec<FieldOrGroup>,
}

impl ConfigurationGroup {
    pub fn name(&self) -> Cow<'_, str> {
        self.rename()
            .map_or_else(|| Cow::Owned(self.ident.to_string()), Cow::Borrowed)
    }

    pub fn rename(&self) -> Option<&str> {
        Some(self.rename.as_ref()?.as_str())
    }
}

impl TryFrom<super::parse::ConfigurationGroup> for ConfigurationGroup {
    type Error = syn::Error;

    fn try_from(mut value: super::parse::ConfigurationGroup) -> Result<Self, Self::Error> {
        deny_attribute(
            &value.attrs,
            "serde",
            "rename",
            "tedge_config(rename)",
            "rename a group",
        )?;
        deny_attribute(
            &value.attrs,
            "serde",
            "alias",
            "tedge_config(deprecated_name)",
            "supply an alias for a group",
        )?;

        for name in value.deprecated_names {
            // TODO similar errors to fields?
            let name_str = name.as_str();
            value
                .attrs
                .push(parse_quote_spanned! {name.span() => #[serde(alias = #name_str)]})
        }

        Ok(Self {
            attrs: value.attrs,
            rename: value.rename,
            dto: value.dto,
            reader: value.reader,
            ident: value.ident,
            contents: combine_errors(value.content.into_iter().map(<_>::try_from))?,
        })
    }
}

#[allow(clippy::large_enum_variant)] // macro code, low impact
#[derive(Debug)]
pub enum FieldOrGroup {
    Field(ConfigurableField),
    Group(ConfigurationGroup),
    Multi(ConfigurationGroup),
}

impl FieldOrGroup {
    pub fn name(&self) -> Cow<'_, str> {
        let rename = match self {
            Self::Group(group) => group.rename.as_ref().map(|s| s.as_str()),
            Self::Multi(group) => group.rename.as_ref().map(|s| s.as_str()),
            Self::Field(field) => field.rename(),
        };

        rename.map_or_else(|| Cow::Owned(self.ident().to_string()), Cow::Borrowed)
    }

    pub fn ident(&self) -> &syn::Ident {
        match self {
            Self::Field(field) => field.ident(),
            Self::Group(group) => &group.ident,
            Self::Multi(group) => &group.ident,
        }
    }

    pub fn is_called(&self, target: &syn::Ident) -> bool {
        self.ident() == target
            || match self {
                Self::Field(field) => field.rename().is_some_and(|rename| *target == rename),
                // Groups don't support renaming at the moment
                Self::Group(_) => false,
                Self::Multi(_) => false,
            }
    }

    pub fn field(&self) -> Option<&ConfigurableField> {
        match self {
            Self::Field(field) => Some(field),
            Self::Group(..) => None,
            Self::Multi(..) => None,
        }
    }

    pub fn reader(&self) -> &ReaderSettings {
        match self {
            Self::Field(field) => field.reader(),
            Self::Group(group) => &group.reader,
            Self::Multi(group) => &group.reader,
        }
    }

    pub fn dto_skip(&self) -> bool {
        match self {
            Self::Field(field) => field.dto().skip,
            Self::Group(group) => group.dto.skip,
            Self::Multi(group) => group.dto.skip,
        }
    }

    pub fn doc(&self) -> Option<String> {
        let attrs = match self {
            Self::Field(field) => field.attrs(),
            Self::Group(group) | Self::Multi(group) => &group.attrs,
        };
        doc_comments_from(attrs)
    }
}

fn doc_comments_from(attrs: &[syn::Attribute]) -> Option<String> {
    let doc = attrs
        .iter()
        .filter(|attr| attr.path() == &parse_quote!(doc))
        .filter_map(|attr| match attr.meta {
            Meta::NameValue(MetaNameValue {
                value:
                    Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(ref lit),
                        ..
                    }),
                ..
            }) => Some(lit.value()),
            _ => None,
        })
        .map(|lit| lit.trim().to_owned())
        .collect::<Vec<_>>()
        .join("\n");
    match doc.as_str() {
        "" => None,
        _ => Some(doc),
    }
}

impl TryFrom<super::parse::FieldOrGroup> for FieldOrGroup {
    type Error = syn::Error;
    fn try_from(value: super::parse::FieldOrGroup) -> Result<Self, Self::Error> {
        match value {
            super::parse::FieldOrGroup::Field(field) => field.try_into().map(Self::Field),
            super::parse::FieldOrGroup::Group(group) if group.multi => {
                group.try_into().map(Self::Multi)
            }
            super::parse::FieldOrGroup::Group(group) => group.try_into().map(Self::Group),
        }
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ConfigurableField {
    ReadOnly(ReadOnlyField),
    ReadWrite(ReadWriteField),
}

#[derive(Debug)]
pub struct ReadOnlyField {
    pub attrs: Vec<syn::Attribute>,
    pub deprecated_keys: Vec<SpannedValue<String>>,
    pub readonly: ReadonlySettings,
    pub rename: Option<SpannedValue<String>>,
    pub dto: FieldDtoSettings,
    pub reader: ReaderSettings,
    pub ident: syn::Ident,
    pub ty: syn::Type,
    pub from: Option<syn::Type>,
}

impl ReadOnlyField {
    pub fn lazy_reader_name(&self, parents: &[PathItem]) -> syn::Ident {
        format_ident!(
            "LazyReader{}{}",
            parents
                .iter()
                .filter_map(PathItem::as_static)
                .map(|p| p.to_string().to_upper_camel_case())
                .collect::<Vec<_>>()
                .join(""),
            self.rename()
                .map(<_>::to_owned)
                .unwrap_or_else(|| self.ident.to_string())
                .to_upper_camel_case()
        )
    }

    pub fn parent_name(&self, parents: &[PathItem]) -> syn::Ident {
        format_ident!(
            "TEdgeConfigReader{}",
            parents
                .iter()
                .filter_map(PathItem::as_static)
                .map(|p| p.to_string().to_upper_camel_case())
                .collect::<Vec<_>>()
                .join(""),
        )
    }

    pub fn rename(&self) -> Option<&str> {
        Some(self.rename.as_ref()?.as_str())
    }
}

impl ReadWriteField {
    pub fn lazy_reader_name(&self, parents: &[PathItem]) -> syn::Ident {
        format_ident!(
            "LazyReader{}{}",
            parents
                .iter()
                .filter_map(PathItem::as_static)
                .map(|p| p.to_string().to_upper_camel_case())
                .collect::<Vec<_>>()
                .join(""),
            self.rename()
                .map(<_>::to_owned)
                .unwrap_or_else(|| self.ident.to_string())
                .to_upper_camel_case()
        )
    }

    pub fn parent_name(&self, parents: &[PathItem]) -> syn::Ident {
        format_ident!(
            "TEdgeConfigReader{}",
            parents
                .iter()
                .filter_map(PathItem::as_static)
                .map(|p| p.to_string().to_upper_camel_case())
                .collect::<Vec<_>>()
                .join(""),
        )
    }

    pub fn rename(&self) -> Option<&str> {
        Some(self.rename.as_ref()?.as_str())
    }

    pub fn dto_ty(&self) -> &syn::Type {
        self.sub_fields
            .as_ref()
            .map(|s| &s.dto_ty)
            .unwrap_or_else(|| &self.ty)
    }
}

#[derive(Debug)]
pub struct ReadWriteField {
    pub attrs: Vec<syn::Attribute>,
    pub deprecated_keys: Vec<SpannedValue<String>>,
    pub rename: Option<SpannedValue<String>>,
    pub dto: FieldDtoSettings,
    pub reader: ReaderSettings,
    pub examples: Vec<SpannedValue<String>>,
    sub_fields: Option<SubFields>,
    pub ident: syn::Ident,
    ty: syn::Type,
    pub default: FieldDefault,
    pub from: Option<syn::Type>,
}

#[derive(Debug)]
struct SubFields {
    value: SpannedValue<Vec<EnumEntry>>,
    dto_ty: syn::Type,
    reader_ty: syn::Type,
}

impl ConfigurableField {
    pub fn attrs(&self) -> &[syn::Attribute] {
        match self {
            Self::ReadOnly(ReadOnlyField { attrs, .. })
            | Self::ReadWrite(ReadWriteField { attrs, .. }) => attrs,
        }
    }

    pub fn name(&self) -> Cow<'_, str> {
        self.rename()
            .map_or_else(|| Cow::Owned(self.ident().to_string()), Cow::Borrowed)
    }

    pub fn has_guaranteed_default(&self) -> bool {
        match self {
            Self::ReadWrite(_) => !self.is_optional(),
            Self::ReadOnly(..) => false,
        }
    }

    pub fn is_optional(&self) -> bool {
        matches!(
            self,
            Self::ReadWrite(ReadWriteField {
                default: FieldDefault::FromOptionalKey(_) | FieldDefault::None,
                ..
            })
        )
    }

    pub fn ident(&self) -> &syn::Ident {
        match self {
            Self::ReadOnly(ReadOnlyField { ident, .. })
            | Self::ReadWrite(ReadWriteField { ident, .. }) => ident,
        }
    }

    pub fn rename(&self) -> Option<&str> {
        match self {
            Self::ReadOnly(ReadOnlyField { rename, .. })
            | Self::ReadWrite(ReadWriteField { rename, .. }) => Some(rename.as_ref()?.as_str()),
        }
    }

    pub fn dto_ty(&self) -> &syn::Type {
        self.sub_fields()
            .map(|s| &s.dto_ty)
            .unwrap_or_else(|| self.ty())
    }

    pub fn reader_ty(&self) -> &syn::Type {
        self.sub_fields()
            .map(|s| &s.reader_ty)
            .unwrap_or_else(|| self.ty())
    }

    fn ty(&self) -> &syn::Type {
        match self {
            Self::ReadOnly(ReadOnlyField { ty, .. })
            | Self::ReadWrite(ReadWriteField { ty, .. }) => ty,
        }
    }

    pub fn dto(&self) -> &FieldDtoSettings {
        match self {
            Self::ReadOnly(ReadOnlyField { dto, .. })
            | Self::ReadWrite(ReadWriteField { dto, .. }) => dto,
        }
    }

    #[allow(unused)]
    pub fn reader(&self) -> &ReaderSettings {
        match self {
            Self::ReadOnly(ReadOnlyField { reader, .. })
            | Self::ReadWrite(ReadWriteField { reader, .. }) => reader,
        }
    }

    pub fn read_write(&self) -> Option<&ReadWriteField> {
        match self {
            Self::ReadWrite(field) => Some(field),
            Self::ReadOnly(_) => None,
        }
    }

    pub fn read_only(&self) -> Option<&ReadOnlyField> {
        match self {
            Self::ReadOnly(field) => Some(field),
            Self::ReadWrite(_) => None,
        }
    }

    pub fn reader_function(&self) -> Option<(&syn::Path, &ReadWriteField)> {
        match self {
            Self::ReadWrite(
                field @ ReadWriteField {
                    reader:
                        ReaderSettings {
                            function: Some(f), ..
                        },
                    ..
                },
            ) => Some((f, field)),
            _ => None,
        }
    }

    pub fn deprecated_keys(&self) -> impl Iterator<Item = &str> {
        let keys = match self {
            Self::ReadOnly(field) => &field.deprecated_keys,
            Self::ReadWrite(field) => &field.deprecated_keys,
        };
        keys.iter().map(|key| key.as_str())
    }

    pub fn from(&self) -> Option<&syn::Type> {
        match self {
            Self::ReadOnly(field) => &field.from,
            Self::ReadWrite(field) => &field.from,
        }
        .as_ref()
    }

    pub fn sub_field_entries(&self) -> Option<&SpannedValue<Vec<EnumEntry>>> {
        self.read_write()
            .and_then(|rw| Some(&rw.sub_fields.as_ref()?.value))
    }

    fn sub_fields(&self) -> Option<&SubFields> {
        self.read_write().and_then(|rw| rw.sub_fields.as_ref())
    }
}

impl TryFrom<super::parse::ConfigurableField> for ConfigurableField {
    type Error = syn::Error;
    fn try_from(mut value: super::parse::ConfigurableField) -> Result<Self, Self::Error> {
        let mut custom_errors = OptionalError::default();

        let attrs = &value.attrs;
        deny_attribute(
            attrs,
            "serde",
            "rename",
            "tedge_config(rename)",
            "rename a field",
        )
        .append_err_to(&mut custom_errors);
        deny_attribute(
            attrs,
            "serde",
            "alias",
            "tedge_config(deprecated_name)",
            "create an alias for a field",
        )
        .append_err_to(&mut custom_errors);
        deny_attribute(
            attrs,
            "doku",
            "example",
            "tedge_config(example)",
            "supply an example value for a field",
        )
        .append_err_to(&mut custom_errors);

        if let Some(renamed_to) = &value.rename {
            let span = renamed_to.span();
            let literal = renamed_to.as_str();
            value
                .attrs
                .push(parse_quote_spanned!(span=> #[serde(rename = #literal)]))
        }

        if value.reader.function.is_some() && value.from.is_none() {
            value.from = Some(
                extract_type_from_result(&value.ty)
                    .map_or(&value.ty, |tys| tys.0)
                    .clone(),
            );
        }

        for name in value.deprecated_names {
            let name_str = name.as_str();
            if name.contains('.') {
                custom_errors.combine(syn::Error::new(
                    name.span(),
                    format!("this a path rather than a field or group name. Did you mean to use #[tedge_config(deprecated_key = \"{name_str}\")] instead?")
                ));
            }
            value
                .attrs
                .push(parse_quote_spanned! {name.span()=> #[serde(alias = #name_str)]})
        }

        for key in &value.deprecated_keys {
            if !key.contains('.') {
                custom_errors.combine(syn::Error::new(
                    key.span(),
                    format!("this is just a field or group name, not a key (which would contain one or more `.`s). Did you mean to use #[tedge_config(deprecated_name = \"{}\"] instead?", key.as_str())
                ));
            }
        }

        for example in &value.examples {
            let example_str = example.as_str();
            value
                .attrs
                .push(parse_quote_spanned! {example.span()=> #[doku(example = #example_str)]});
        }

        if let Some(note) = value.note {
            value.attrs.push(tedge_note_to_doku_meta(&note));
        }

        let string_like: [syn::Type; 5] = [
            parse_quote!(Arc<str>),
            parse_quote!(::std::sync::Arc<str>),
            parse_quote!(std::sync::Arc<str>),
            parse_quote!(core::sync::Arc<str>),
            parse_quote!(sync::Arc<str>),
        ];

        if value.from.is_none() && string_like.contains(&value.ty) {
            value.from = Some(parse_quote!(::std::string::String));
        }

        let res = if let Some(readonly) = value.readonly {
            if let Some(sub_fields) = &value.sub_fields {
                custom_errors.combine(syn::Error::new(
                    sub_fields.span(),
                    "read-only fields cannot have sub-fields",
                ));
            }
            Self::ReadOnly(ReadOnlyField {
                attrs: value.attrs,
                deprecated_keys: value.deprecated_keys,
                rename: value.rename,
                ident: value.ident.unwrap(),
                readonly,
                ty: value.ty,
                dto: value.dto,
                reader: value.reader,
                from: value.from,
            })
        } else {
            let sub_fields = match value.sub_fields {
                Some(sf) => {
                    let error =
                        "The type name for a sub-field enum must be an identifier, e.g. C8y";
                    let syn::Type::Path(syn::TypePath { path, .. }) = &value.ty else {
                        return Err(syn::Error::new(value.ty.span(), error));
                    };
                    let Some(ident) = path.get_ident() else {
                        return Err(syn::Error::new(value.ty.span(), error));
                    };
                    Some(SubFields {
                        dto_ty: syn::Type::Path(syn::TypePath {
                            qself: None,
                            path: format_ident!("{ident}Dto").into(),
                        }),
                        reader_ty: syn::Type::Path(syn::TypePath {
                            qself: None,
                            path: format_ident!("{ident}Reader").into(),
                        }),
                        value: sf.map_ref(|s| s.0.clone()),
                    })
                }
                None => None,
            };
            Self::ReadWrite(ReadWriteField {
                attrs: value.attrs,
                deprecated_keys: value.deprecated_keys,
                rename: value.rename,
                examples: value.examples,
                sub_fields,
                ident: value.ident.unwrap(),
                ty: value.ty,
                dto: value.dto,
                reader: value.reader,
                default: value.default.unwrap_or(FieldDefault::None),
                from: value.from,
            })
        };

        custom_errors.try_throw()?;
        Ok(res)
    }
}

fn deny_attribute(
    attrs: &[syn::Attribute],
    krate: &str,
    attribute: &str,
    our_name: &str,
    action: &str,
) -> Result<(), syn::Error> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident(krate))
        .filter_map(|attr| attr.meta.require_list().ok())
        .filter_map(|attr| darling::ast::NestedMeta::parse_meta_list(attr.tokens.clone()).ok())
        .flatten()
        .filter_map(|attr| match attr {
            NestedMeta::Meta(m) => Some(m),
            _ => None,
        })
        .filter_map(|meta| Some(meta.require_name_value().ok()?.to_owned()))
        .filter(|attr| attr.path.is_ident(attribute))
        .map(|attr| {
            syn::Error::new(
                attr.span(),
                format!("use #[{our_name}] instead of #[{krate}({attribute})] to {action}"),
            )
        })
        .fold(OptionalError::default(), |errors, e| {
            errors.combine_owned(e)
        })
        .try_throw()
}

fn tedge_note_to_doku_meta(note: &SpannedValue<String>) -> syn::Attribute {
    let meta = format!("note = {}", note.as_str());
    parse_quote_spanned!(note.span()=> #[doku(meta(#meta))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::Span;
    use quote::quote;
    use syn::parse_quote;

    #[test]
    fn doku_examples_are_denied() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            device: {
                #[doku(example = "test")]
                id: String,
            },
        })
        .unwrap();

        assert!(Configuration::try_from(input).is_err());
    }

    #[test]
    fn tedge_note_is_converted_to_doku_meta() {
        let note = SpannedValue::new("A note".to_owned(), Span::call_site());
        assert_eq!(
            tedge_note_to_doku_meta(&note),
            parse_quote!(
                #[doku(meta("note = A note"))]
            )
        );
    }

    #[test]
    fn serde_rename_is_denied_for_fields() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            device: {
                #[serde(rename = "type")]
                ty: String,
            },
        })
        .unwrap();

        let error = Configuration::try_from(input).unwrap_err();
        assert!(error.to_string().contains("#[tedge_config(rename)]"))
    }

    #[test]
    fn serde_alias_is_denied_for_fields() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            device: {
                #[serde(alias = "type")]
                ty: String,
            },
        })
        .unwrap();

        let error = Configuration::try_from(input).unwrap_err();
        assert!(error
            .to_string()
            .contains("#[tedge_config(deprecated_name)]"))
    }

    #[test]
    fn serde_alias_is_denied_for_groups() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            #[serde(alias = "dev")]
            device: {
                ty: String,
            },
        })
        .unwrap();

        let error = Configuration::try_from(input).unwrap_err();
        assert!(error
            .to_string()
            .contains("#[tedge_config(deprecated_name)]"))
    }

    #[test]
    fn deprecated_key_accepts_valid_keys_for_fields() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            mqtt: {
                bind: {
                    #[tedge_config(deprecated_key = "mqtt.port")]
                    port: u16,
                }
            },
        })
        .unwrap();

        Configuration::try_from(input).unwrap();
    }

    #[test]
    fn deprecated_name_accepts_valid_names_for_fields() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            mqtt: {
                auth: {
                    #[tedge_config(deprecated_name = "cafile")]
                    ca_file: u16,
                }
            },
        })
        .unwrap();

        Configuration::try_from(input).unwrap();
    }

    #[test]
    fn rename_accepts_valid_keys_for_groups() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            mqtt: {
                #[tedge_config(rename = "notbind")]
                bind: {
                    port: u16,
                }
            },
        })
        .unwrap();

        Configuration::try_from(input).unwrap();
    }

    #[test]
    fn deprecated_name_accepts_valid_names_for_groups() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            mqtt: {
                #[tedge_config(deprecated_name = "old_auth")]
                auth: {
                    ca_file: u16,
                }
            },
        })
        .unwrap();

        Configuration::try_from(input).unwrap();
    }

    #[test]
    fn group_name_is_derived_from_ident_if_not_renamed() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            c8y: {
                url: String,
            }
        })
        .unwrap();

        let configuration = Configuration::try_from(input).unwrap();

        assert_eq!(configuration.groups[0].name(), "c8y")
    }

    #[test]
    fn group_can_be_renamed() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            #[tedge_config(rename = "cumulocity")]
            c8y: {
                url: String,
            }
        })
        .unwrap();

        let configuration = Configuration::try_from(input).unwrap();

        assert_eq!(configuration.groups[0].name(), "cumulocity")
    }

    #[test]
    fn field_name_is_derived_from_ident_if_not_renamed() {
        let input: super::super::parse::ConfigurableField = syn::parse2(quote! {
            ty: String
        })
        .unwrap();

        let field = FieldOrGroup::Field(ConfigurableField::try_from(input).unwrap());

        assert_eq!(field.name(), "ty")
    }

    #[test]
    fn field_can_be_renamed() {
        let input: super::super::parse::ConfigurableField = syn::parse2(quote! {
            #[tedge_config(rename = "type")]
            ty: String
        })
        .unwrap();

        let field = FieldOrGroup::Field(ConfigurableField::try_from(input).unwrap());

        assert_eq!(field.name(), "type")
    }

    #[test]
    fn sub_config_rejects_multi_profile_groups() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            #[tedge_config(multi)]
            c8y: {
                url: String,
            }
        })
        .unwrap();

        let config = Configuration::try_from(input).unwrap();
        let error = config.validate_for_sub_config().unwrap_err();
        assert!(error.to_string().contains("Multi-profile groups"));
    }

    #[test]
    fn sub_config_rejects_nested_multi_profile_groups() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            bridge: {
                #[tedge_config(multi)]
                profiles: {
                    url: String,
                }
            }
        })
        .unwrap();

        let config = Configuration::try_from(input).unwrap();
        let error = config.validate_for_sub_config().unwrap_err();
        assert!(error.to_string().contains("Multi-profile groups"));
    }

    #[test]
    fn sub_config_accepts_regular_groups() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            bridge_azure: {
                url: String,
            },
            bridge_aws: {
                region: String,
            }
        })
        .unwrap();

        let config = Configuration::try_from(input).unwrap();
        assert!(config.validate_for_sub_config().is_ok());
    }
}
