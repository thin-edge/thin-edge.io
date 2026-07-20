//! The contract between `define_config!` and the runtime.
//!
//! Every `define_config!` invocation implements [ConfigSchema] on its
//! generated reader type. The trait links the reader to its DTO type and
//! exposes the schema's registry data as plain values, so a schema can be
//! mounted inside another one (`device: extern MapperDeviceConfig`) by
//! prefixing that data with the mount key.

use std::borrow::Cow;
use std::path::Path;

use facet::Facet;

use crate::append_remove::AppendRemoveRegistry;
use crate::defaults::{DefaultSpec, FieldDefault};

/// A configuration schema defined by `define_config!`.
///
/// Implemented on the generated reader type; `Dto` is the matching
/// serialization type. Read-only markers, deprecated key aliases, and
/// example values live as facet attributes on the DTO fields and are
/// discovered at runtime through shape-tree walks.
pub trait ConfigSchema {
    type Dto: for<'a> Facet<'a> + Default + Clone + serde::Serialize + serde::de::DeserializeOwned;

    /// Defaulting rules for every key in the schema
    fn defaults(config_dir: &Path) -> Vec<FieldDefault>;

    /// Registers the schema's leaf types with the append/remove registry
    fn register_types(registry: &mut AppendRemoveRegistry);
}

/// Remaps a mounted schema's defaults into the mounting schema's key space.
///
/// Prefixes each field key and every same-schema source key (`from_key`,
/// `from_optional_key`, `from_key_via`). `from_root` keys name keys of the
/// root config, not the mounted schema, so they are left untouched.
pub fn prefix_defaults(prefix: &str, defaults: Vec<FieldDefault>) -> Vec<FieldDefault> {
    defaults
        .into_iter()
        .map(|d| FieldDefault {
            key: prefix_key(prefix, d.key),
            spec: match d.spec {
                DefaultSpec::FromKey(source) => DefaultSpec::FromKey(prefix_key(prefix, source)),
                DefaultSpec::FromOptionalKey(source) => {
                    DefaultSpec::FromOptionalKey(prefix_key(prefix, source))
                }
                DefaultSpec::FromKeyVia { key, function } => DefaultSpec::FromKeyVia {
                    key: prefix_key(prefix, key),
                    function,
                },
                spec @ (DefaultSpec::Value(_)
                | DefaultSpec::Function(_)
                | DefaultSpec::FromRoot(_)) => spec,
            },
        })
        .collect()
}

fn prefix_key(prefix: &str, key: Cow<'static, str>) -> Cow<'static, str> {
    Cow::Owned(format!("{prefix}.{key}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_keys_are_prefixed() {
        let prefixed = prefix_defaults(
            "device",
            vec![FieldDefault {
                key: "id".into(),
                spec: DefaultSpec::Value("thin-edge".into()),
            }],
        );
        assert_eq!(prefixed[0].key, "device.id");
    }

    #[test]
    fn same_schema_source_keys_are_prefixed() {
        let prefixed = prefix_defaults(
            "device",
            vec![
                FieldDefault {
                    key: "key_path".into(),
                    spec: DefaultSpec::FromKey("cert_path".into()),
                },
                FieldDefault {
                    key: "csr_path".into(),
                    spec: DefaultSpec::FromOptionalKey("cert_path".into()),
                },
                FieldDefault {
                    key: "id".into(),
                    spec: DefaultSpec::FromKeyVia {
                        key: "cert_path".into(),
                        function: |_| Ok(None),
                    },
                },
            ],
        );
        let sources: Vec<&str> = prefixed
            .iter()
            .map(|d| match &d.spec {
                DefaultSpec::FromKey(s)
                | DefaultSpec::FromOptionalKey(s)
                | DefaultSpec::FromKeyVia { key: s, .. } => s.as_ref(),
                other => panic!("unexpected spec: {other:?}"),
            })
            .collect();
        assert_eq!(
            sources,
            ["device.cert_path", "device.cert_path", "device.cert_path"]
        );
    }

    #[test]
    fn from_root_keys_are_not_remapped() {
        let prefixed = prefix_defaults(
            "device",
            vec![FieldDefault {
                key: "cert_path".into(),
                spec: DefaultSpec::FromRoot("device.cert_path"),
            }],
        );
        assert_eq!(prefixed[0].key, "device.cert_path");
        assert!(
            matches!(&prefixed[0].spec, DefaultSpec::FromRoot("device.cert_path")),
            "from_root key must stay untouched"
        );
    }
}
