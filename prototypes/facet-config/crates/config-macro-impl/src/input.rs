use darling::FromAttributes;
use darling::FromField;
use darling::FromMeta;
use syn::parse::Parse;
use syn::punctuated::Punctuated;
use syn::Attribute;
use syn::Token;

#[derive(Debug)]
pub struct Configuration {
    pub name: syn::Ident,
    pub groups: Punctuated<FieldOrGroup, Token![,]>,
}

#[derive(Debug)]
pub enum FieldOrGroup {
    Field(Box<ConfigField>),
    Group(ConfigGroup),
}

#[derive(Debug)]
pub struct ConfigGroup {
    pub doc_attrs: Vec<syn::Attribute>,
    pub ident: syn::Ident,
    pub contents: Punctuated<FieldOrGroup, Token![,]>,
}

#[derive(FromField, Debug)]
#[darling(attributes(tedge_config), forward_attrs(doc))]
pub struct ConfigField {
    pub attrs: Vec<syn::Attribute>,
    #[darling(default)]
    pub readonly: bool,
    #[darling(default)]
    pub rename: Option<String>,
    #[darling(default)]
    pub deprecated_key: Option<String>,
    #[darling(default)]
    pub default: Option<FieldDefault>,
    #[darling(multiple, rename = "example")]
    pub examples: Vec<String>,
    pub ident: Option<syn::Ident>,
    pub ty: syn::Type,
}

#[derive(Debug, FromMeta)]
pub enum FieldDefault {
    Value(String),
    Function(syn::Path),
    FromKey(String),
    FromOptionalKey(String),
    FromConfigDir(String),
    FromRoot(String),
    FromKeyVia(FromKeyVia),
}

/// A default derived from another key's resolved value via a function
#[derive(Debug, FromMeta)]
pub struct FromKeyVia {
    pub key: String,
    pub function: syn::Path,
}

/// Groups accept no `tedge_config` attributes; parsing this rejects any
/// that are present with an error naming the unknown attribute
#[derive(FromAttributes, Debug)]
#[darling(attributes(tedge_config))]
struct GroupAttributes {}

impl Parse for Configuration {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name: syn::Ident = input.parse()?;
        let content;
        syn::braced!(content in input);
        let groups = content.parse_terminated(<_>::parse, Token![,])?;
        Ok(Self { name, groups })
    }
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

impl Parse for ConfigGroup {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let content;
        let attrs = input.call(Attribute::parse_outer)?;
        GroupAttributes::from_attributes(&attrs)
            .map_err(|e| syn::Error::new(e.span(), e.to_string()))?;
        let doc_attrs = attrs.into_iter().filter(is_doc_attr).collect();
        let ident = input.parse()?;
        let _colon: Token![:] = input.parse()?;
        let _brace = syn::braced!(content in input);
        let contents = content.parse_terminated(<_>::parse, Token![,])?;
        Ok(ConfigGroup {
            doc_attrs,
            ident,
            contents,
        })
    }
}

impl Parse for ConfigField {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Self::from_field(&input.call(syn::Field::parse_named)?)
            .map_err(|e| syn::Error::new(e.span(), e.to_string()))
    }
}

impl ConfigField {
    pub fn config_name(&self) -> String {
        self.rename
            .clone()
            .unwrap_or_else(|| self.ident.as_ref().unwrap().to_string())
    }
}

impl ConfigGroup {
    pub fn config_name(&self) -> String {
        self.ident.to_string()
    }
}

fn is_doc_attr(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("doc")
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn parse_simple_group_with_fields() {
        let config: Configuration = parse_quote!(
            Test {
                mqtt: {
                    port: u16,
                    host: String,
                },
            }
        );
        assert_eq!(config.groups.len(), 1);
        match &config.groups[0] {
            FieldOrGroup::Group(g) => {
                assert_eq!(g.ident, "mqtt");
                assert_eq!(g.contents.len(), 2);
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn parse_nested_groups() {
        let config: Configuration = parse_quote!(
            Test {
                c8y: {
                    url: String,
                    proxy: {
                        port: u16,
                    },
                },
            }
        );
        match &config.groups[0] {
            FieldOrGroup::Group(g) => {
                assert_eq!(g.contents.len(), 2);
                match &g.contents[1] {
                    FieldOrGroup::Group(nested) => assert_eq!(nested.ident, "proxy"),
                    _ => panic!("expected nested group"),
                }
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn parse_field_attributes() {
        let config: Configuration = parse_quote!(
            Test {
                mqtt: {
                    #[tedge_config(default(value = "1883"), readonly)]
                    port: u16,

                    #[tedge_config(rename = "type")]
                    ty: String,

                    #[tedge_config(deprecated_key = "mqtt.external.host")]
                    host: String,
                },
            }
        );
        match &config.groups[0] {
            FieldOrGroup::Group(g) => {
                match &g.contents[0] {
                    FieldOrGroup::Field(f) => {
                        assert!(f.readonly);
                        assert!(matches!(&f.default, Some(FieldDefault::Value(v)) if v == "1883"));
                    }
                    _ => panic!("expected field"),
                }
                match &g.contents[1] {
                    FieldOrGroup::Field(f) => {
                        assert_eq!(f.rename.as_deref(), Some("type"));
                    }
                    _ => panic!("expected field"),
                }
                match &g.contents[2] {
                    FieldOrGroup::Field(f) => {
                        assert_eq!(f.deprecated_key.as_deref(), Some("mqtt.external.host"));
                    }
                    _ => panic!("expected field"),
                }
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn group_attributes_are_rejected() {
        let err = syn::parse_str::<Configuration>(
            "Test {
                #[tedge_config(multi)]
                c8y: {
                    url: String,
                },
            }",
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("Unknown field: `multi`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parse_doc_comments_preserved() {
        let config: Configuration = parse_quote!(
            Test {
                mqtt: {
                    /// MQTT broker port
                    port: u16,
                },
            }
        );
        match &config.groups[0] {
            FieldOrGroup::Group(g) => match &g.contents[0] {
                FieldOrGroup::Field(f) => {
                    assert_eq!(f.attrs.len(), 1);
                    assert!(f.attrs[0].path().is_ident("doc"));
                }
                _ => panic!("expected field"),
            },
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn parse_default_variants() {
        let config: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(default(from_key = "other.field"))]
                    cert_path: String,

                    #[tedge_config(default(from_config_dir = "certs/cert.pem"))]
                    key_path: String,

                    #[tedge_config(default(from_optional_key = "c8y.url"))]
                    url: String,
                },
            }
        );
        match &config.groups[0] {
            FieldOrGroup::Group(g) => {
                match &g.contents[0] {
                    FieldOrGroup::Field(f) => {
                        assert!(
                            matches!(&f.default, Some(FieldDefault::FromKey(k)) if k == "other.field")
                        );
                    }
                    _ => panic!("expected field"),
                }
                match &g.contents[1] {
                    FieldOrGroup::Field(f) => {
                        assert!(
                            matches!(&f.default, Some(FieldDefault::FromConfigDir(p)) if p == "certs/cert.pem")
                        );
                    }
                    _ => panic!("expected field"),
                }
                match &g.contents[2] {
                    FieldOrGroup::Field(f) => {
                        assert!(
                            matches!(&f.default, Some(FieldDefault::FromOptionalKey(k)) if k == "c8y.url")
                        );
                    }
                    _ => panic!("expected field"),
                }
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn parse_from_key_via_default() {
        let config: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(default(from_key_via(
                        key = "device.cert_path",
                        function = "device_id_from_cert"
                    )))]
                    id: String,
                },
            }
        );
        match &config.groups[0] {
            FieldOrGroup::Group(g) => match &g.contents[0] {
                FieldOrGroup::Field(f) => match &f.default {
                    Some(FieldDefault::FromKeyVia(via)) => {
                        assert_eq!(via.key, "device.cert_path");
                        assert!(via.function.is_ident("device_id_from_cert"));
                    }
                    other => panic!("expected from_key_via default, got {other:?}"),
                },
                _ => panic!("expected field"),
            },
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn parse_example_attributes() {
        let config: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(example = "my-device", example = "AINA123")]
                    id: String,

                    name: String,
                },
            }
        );
        match &config.groups[0] {
            FieldOrGroup::Group(g) => {
                match &g.contents[0] {
                    FieldOrGroup::Field(f) => {
                        assert_eq!(f.examples, vec!["my-device", "AINA123"]);
                    }
                    _ => panic!("expected field"),
                }
                match &g.contents[1] {
                    FieldOrGroup::Field(f) => {
                        assert!(f.examples.is_empty());
                    }
                    _ => panic!("expected field"),
                }
            }
            _ => panic!("expected group"),
        }
    }
}
