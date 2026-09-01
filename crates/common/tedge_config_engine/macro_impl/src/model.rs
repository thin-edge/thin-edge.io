//! Resolves a parsed config declaration into the shared view used by all
//! generators.
//!
//! The parsed input follows the groups as written. This model adds generated
//! type names and complete config keys, including renamed fields, so the DTO,
//! reader, and runtime schema stay in sync.

use heck::ToUpperCamelCase;
use quote::format_ident;
use syn::punctuated::Punctuated;
use syn::Token;

use crate::input::ConfigExternalGroup;
use crate::input::ConfigField;
use crate::input::Configuration;
use crate::input::FieldOrGroup;

pub struct Model<'a> {
    pub root: GroupModel<'a>,
}

/// One config group as seen by each generator.
pub struct GroupModel<'a> {
    /// The optional form used to load and edit stored config.
    pub dto_ident: syn::Ident,
    /// The typed form returned to application code.
    pub reader_ident: syn::Ident,
    pub items: Vec<ItemModel<'a>>,
}

pub enum ItemModel<'a> {
    Field(FieldModel<'a>),
    Group(ChildGroup<'a>),
    External(ExternalModel<'a>),
}

pub struct FieldModel<'a> {
    pub field: &'a ConfigField,
    /// The key accepted by runtime config operations.
    pub key: String,
}

pub struct ChildGroup<'a> {
    pub ident: &'a syn::Ident,
    pub doc_attrs: &'a [syn::Attribute],
    pub group: GroupModel<'a>,
}

pub struct ExternalModel<'a> {
    pub ext: &'a ConfigExternalGroup,
    /// Where the external schema's keys appear in this config.
    pub prefix: String,
}

// Retain source spans so errors in generated types point to the caller's
// declaration.
impl<'a> Model<'a> {
    pub fn new(config: &'a Configuration) -> Self {
        let name = config.name.to_string();
        Self {
            root: GroupModel {
                dto_ident: format_ident!("{name}ConfigDto", span = config.name.span()),
                reader_ident: format_ident!("{name}Config", span = config.name.span()),
                items: lower_items(&config.groups, "", ""),
            },
        }
    }
}

impl<'a> GroupModel<'a> {
    /// Fields whose runtime metadata is defined by this macro call.
    ///
    /// External groups provide their own metadata through `ConfigSchema`.
    pub fn fields(&self) -> Vec<&FieldModel<'a>> {
        let mut fields = Vec::new();
        self.collect(&mut fields, &mut Vec::new());
        fields
    }

    /// External schemas whose metadata must be included under a local key.
    pub fn externals(&self) -> Vec<&ExternalModel<'a>> {
        let mut externals = Vec::new();
        self.collect(&mut Vec::new(), &mut externals);
        externals
    }

    /// All generated struct idents (DTO + reader) in the tree.
    pub fn all_idents(&self) -> Vec<&syn::Ident> {
        let mut idents = vec![&self.dto_ident, &self.reader_ident];
        for item in &self.items {
            if let ItemModel::Group(child) = item {
                idents.extend(child.group.all_idents());
            }
        }
        idents
    }

    fn collect<'m>(
        &'m self,
        fields: &mut Vec<&'m FieldModel<'a>>,
        externals: &mut Vec<&'m ExternalModel<'a>>,
    ) {
        for item in &self.items {
            match item {
                ItemModel::Field(f) => fields.push(f),
                ItemModel::Group(child) => child.group.collect(fields, externals),
                ItemModel::External(ext) => externals.push(ext),
            }
        }
    }
}

fn lower_items<'a>(
    items: &'a Punctuated<FieldOrGroup, Token![,]>,
    struct_prefix: &str,
    key_prefix: &str,
) -> Vec<ItemModel<'a>> {
    items
        .iter()
        .map(|item| match item {
            FieldOrGroup::Field(f) => ItemModel::Field(FieldModel {
                key: dotted_key(key_prefix, &f.config_name()),
                field: f,
            }),
            FieldOrGroup::Group(g) => {
                let base = struct_name_for_group(struct_prefix, &g.ident.to_string());
                let child_struct_prefix = if struct_prefix.is_empty() {
                    g.ident.to_string()
                } else {
                    format!("{struct_prefix}_{}", g.ident)
                };
                let child_key_prefix = dotted_key(key_prefix, &g.config_name());
                ItemModel::Group(ChildGroup {
                    ident: &g.ident,
                    doc_attrs: &g.doc_attrs,
                    group: GroupModel {
                        dto_ident: format_ident!("{base}ConfigDto", span = g.ident.span()),
                        reader_ident: format_ident!("{base}Config", span = g.ident.span()),
                        items: lower_items(&g.contents, &child_struct_prefix, &child_key_prefix),
                    },
                })
            }
            FieldOrGroup::ExternalGroup(e) => ItemModel::External(ExternalModel {
                prefix: dotted_key(key_prefix, &e.config_name()),
                ext: e,
            }),
        })
        .collect()
}

fn struct_name_for_group(parent_prefix: &str, group_name: &str) -> String {
    if parent_prefix.is_empty() {
        group_name.to_upper_camel_case()
    } else {
        format!(
            "{}{}",
            parent_prefix.to_upper_camel_case(),
            group_name.to_upper_camel_case()
        )
    }
}

fn dotted_key(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}.{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn field_keys_are_dotted_paths() {
        let config: Configuration = parse_quote!(
            Test {
                c8y: {
                    proxy: {
                        port: u16,
                    },
                },
            }
        );
        let model = Model::new(&config);
        let fields = model.root.fields();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].key, "c8y.proxy.port");
    }

    #[test]
    fn renamed_fields_use_the_rename_in_their_key() {
        let config: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(rename = "type")]
                    ty: String,
                },
            }
        );
        let model = Model::new(&config);
        assert_eq!(model.root.fields()[0].key, "device.type");
    }

    #[test]
    fn nested_group_struct_idents_accumulate_parent_names() {
        let config: Configuration = parse_quote!(
            Test {
                c8y: {
                    proxy: {
                        port: u16,
                    },
                },
            }
        );
        let model = Model::new(&config);
        assert_eq!(model.root.dto_ident, "TestConfigDto");
        assert_eq!(model.root.reader_ident, "TestConfig");
        let ItemModel::Group(c8y) = &model.root.items[0] else {
            panic!("expected group");
        };
        assert_eq!(c8y.group.dto_ident, "C8yConfigDto");
        assert_eq!(c8y.group.reader_ident, "C8yConfig");
        let ItemModel::Group(proxy) = &c8y.group.items[0] else {
            panic!("expected group");
        };
        assert_eq!(proxy.group.dto_ident, "C8yProxyConfigDto");
        assert_eq!(proxy.group.reader_ident, "C8yProxyConfig");
    }

    #[test]
    fn external_mount_prefixes_are_dotted_paths() {
        let config: Configuration = parse_quote!(
            Mapper {
                device: extern MapperDeviceConfig,
                c8y: {
                    device: extern MapperDeviceConfig,
                },
            }
        );
        let model = Model::new(&config);
        let externals = model.root.externals();
        assert_eq!(externals.len(), 2);
        assert_eq!(externals[0].prefix, "device");
        assert_eq!(externals[1].prefix, "c8y.device");
    }

    #[test]
    fn fields_exclude_external_schema_contents() {
        let config: Configuration = parse_quote!(
            Mapper {
                url: String,
                device: extern MapperDeviceConfig,
            }
        );
        let model = Model::new(&config);
        let fields = model.root.fields();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].key, "url");
    }
}
