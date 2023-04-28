use darling::util::SpannedValue;
use optional_error::OptionalError;

use crate::utils::combine_errors;

pub use super::parse::{FieldDefault, FieldDtoSettings, GroupDtoSettings, ReaderSettings};

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
    pub rename: Option<String>,
    pub dto: FieldDtoSettings,
    pub reader: ReaderSettings,
    pub ident: syn::Ident,
    pub ty: syn::Type,
}

#[derive(Debug)]
pub struct ReadWriteField {
    pub attrs: Vec<syn::Attribute>,
    pub rename: Option<String>,
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
            | Self::ReadWrite(ReadWriteField { rename, .. }) => rename.as_deref(),
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
    fn try_from(value: super::parse::ConfigurableField) -> Result<Self, Self::Error> {
        if *value.readonly {
            let mut error = OptionalError::default();
            for example in &value.examples {
                error.combine(syn::Error::new(
                    example.span(),
                    "Cannot use `example` on read only field",
                ))
            }

            error.try_throw().map(|_| {
                Self::ReadOnly(ReadOnlyField {
                    attrs: value.attrs,
                    rename: value.rename,
                    ident: value.ident.unwrap(),
                    ty: value.ty,
                    dto: value.dto,
                    reader: value.reader,
                })
            })
        } else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn examples_denied_for_read_only_fields() {
        let input: super::super::parse::Configuration = syn::parse2(quote! {
            device: {
                #[tedge_config(readonly, example = "test")]
                id: String,
            },
        })
        .unwrap();

        assert!(Configuration::try_from(input).is_err());
    }
}
