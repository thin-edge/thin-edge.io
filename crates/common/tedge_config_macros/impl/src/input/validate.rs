use std::borrow::Cow;

use darling::export::NestedMeta;
use darling::util::SpannedValue;
use heck::ToUpperCamelCase;
use quote::format_ident;
use syn::parse_quote_spanned;
use syn::spanned::Spanned;

use crate::error::combine_errors;
use crate::optional_error::OptionalError;

pub use super::parse::FieldDefault;
pub use super::parse::FieldDtoSettings;
pub use super::parse::GroupDtoSettings;
pub use super::parse::ReaderSettings;
use super::parse::ReadonlySettings;

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

#[derive(Debug)]
pub struct ConfigurationGroup {
    pub attrs: Vec<syn::Attribute>,
    pub rename: Option<SpannedValue<String>>,
    pub dto: GroupDtoSettings,
    pub reader: ReaderSettings,
    pub ident: syn::Ident,
    pub contents: Vec<FieldOrGroup>,
}

impl TryFrom<super::parse::ConfigurationGroup> for ConfigurationGroup {
    type Error = syn::Error;

    fn try_from(value: super::parse::ConfigurationGroup) -> Result<Self, Self::Error> {
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

#[derive(Debug)]
pub enum FieldOrGroup {
    Field(ConfigurableField),
    Group(ConfigurationGroup),
}

impl FieldOrGroup {
    pub fn name(&self) -> Cow<str> {
        let rename = match self {
            Self::Group(group) => group.rename.as_ref().map(|s| s.as_str()),
            Self::Field(field) => field.rename(),
        };

        rename.map_or_else(|| Cow::Owned(self.ident().to_string()), Cow::Borrowed)
    }

    pub fn ident(&self) -> &syn::Ident {
        match self {
            Self::Field(field) => field.ident(),
            Self::Group(group) => &group.ident,
        }
    }

    pub fn is_called(&self, target: &syn::Ident) -> bool {
        self.ident() == target
            || match self {
                Self::Field(field) => field.rename().map_or(false, |rename| *target == rename),
                // Groups don't support renaming at the moment
                Self::Group(_) => false,
            }
    }

    pub fn field(&self) -> Option<&ConfigurableField> {
        match self {
            Self::Field(field) => Some(field),
            Self::Group(..) => None,
        }
    }
}

impl TryFrom<super::parse::FieldOrGroup> for FieldOrGroup {
    type Error = syn::Error;
    fn try_from(value: super::parse::FieldOrGroup) -> Result<Self, Self::Error> {
        match value {
            super::parse::FieldOrGroup::Field(field) => field.try_into().map(Self::Field),
            super::parse::FieldOrGroup::Group(group) => group.try_into().map(Self::Group),
        }
    }
}

#[derive(Debug)]
pub enum ConfigurableField {
    ReadOnly(ReadOnlyField),
    ReadWrite(ReadWriteField),
}

#[derive(Debug)]
pub struct ReadOnlyField {
    pub attrs: Vec<syn::Attribute>,
    pub readonly: ReadonlySettings,
    pub rename: Option<SpannedValue<String>>,
    pub dto: FieldDtoSettings,
    pub reader: ReaderSettings,
    pub ident: syn::Ident,
    pub ty: syn::Type,
}

impl ReadOnlyField {
    pub fn lazy_reader_name(&self, parents: &[syn::Ident]) -> syn::Ident {
        format_ident!(
            "LazyReader{}{}",
            parents
                .iter()
                .map(|p| p.to_string().to_upper_camel_case())
                .collect::<Vec<_>>()
                .join("."),
            self.rename()
                .map(<_>::to_owned)
                .unwrap_or_else(|| self.ident.to_string())
                .to_upper_camel_case()
        )
    }

    pub fn rename(&self) -> Option<&str> {
        Some(self.rename.as_ref()?.as_str())
    }
}

#[derive(Debug)]
pub struct ReadWriteField {
    pub attrs: Vec<syn::Attribute>,
    pub rename: Option<SpannedValue<String>>,
    pub dto: FieldDtoSettings,
    pub reader: ReaderSettings,
    pub examples: Vec<SpannedValue<String>>,
    pub ident: syn::Ident,
    pub ty: syn::Type,
    pub default: FieldDefault,
}

impl ConfigurableField {
    pub fn attrs(&self) -> &[syn::Attribute] {
        match self {
            Self::ReadOnly(ReadOnlyField { attrs, .. })
            | Self::ReadWrite(ReadWriteField { attrs, .. }) => attrs,
        }
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

    pub fn ty(&self) -> &syn::Type {
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
}

impl TryFrom<super::parse::ConfigurableField> for ConfigurableField {
    type Error = syn::Error;
    fn try_from(mut value: super::parse::ConfigurableField) -> Result<Self, Self::Error> {
        value
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("serde"))
            .filter_map(|attr| attr.meta.require_list().ok())
            .filter_map(|attr| darling::ast::NestedMeta::parse_meta_list(attr.tokens.clone()).ok())
            .flatten()
            .filter_map(|attr| match attr {
                NestedMeta::Meta(m) => Some(m),
                _ => None,
            })
            .filter_map(|meta| Some(meta.require_name_value().ok()?.to_owned()))
            .filter(|attr| attr.path.is_ident("rename"))
            .map(|attr| {
                syn::Error::new(
                    attr.span(),
                    "use #[tedge_config(rename)] instead of #[serde(rename)] to rename fields",
                )
            })
            .fold(OptionalError::default(), |errors, e| {
                errors.combine_owned(e)
            })
            .try_throw()?;

        if let Some(readonly) = value.readonly {
            let mut error = OptionalError::default();
            for example in &value.examples {
                error.combine(syn::Error::new(
                    example.span(),
                    "Cannot use `example` on read only field",
                ))
            }

            if let Some(note) = value.note {
                value.attrs.push(tedge_note_to_doku_meta(&note));
            }

            error.try_throw().map(|_| {
                Self::ReadOnly(ReadOnlyField {
                    attrs: value.attrs,
                    rename: value.rename,
                    ident: value.ident.unwrap(),
                    readonly,
                    ty: value.ty,
                    dto: value.dto,
                    reader: value.reader,
                })
            })
        } else {
            if let Some(note) = value.note {
                value.attrs.push(tedge_note_to_doku_meta(&note));
            }

            Ok(Self::ReadWrite(ReadWriteField {
                attrs: value.attrs,
                rename: value.rename,
                examples: value.examples,
                ident: value.ident.unwrap(),
                ty: value.ty,
                dto: value.dto,
                reader: value.reader,
                default: value.default.unwrap_or(FieldDefault::None),
            }))
        }
    }
}

fn tedge_note_to_doku_meta(note: &SpannedValue<String>) -> syn::Attribute {
    let meta = format!("note = {}", note.as_str());
    parse_quote_spanned!(note.span()=> #[doku(meta(#meta))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn examples_denied_for_read_only_fields() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            device: {
                #[tedge_config(readonly(write_error = "Field is read only", function = "device_id"), example = "test")]
                id: String,
            },
        })
        .unwrap();

        assert!(Configuration::try_from(input).is_err());
    }
}
