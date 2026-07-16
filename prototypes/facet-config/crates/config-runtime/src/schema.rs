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
use crate::reflect::DeprecatedKey;

/// Example values for a config key, as `(key, examples)` pairs.
pub type KeyExamples = (Cow<'static, str>, &'static [&'static str]);

/// A configuration schema defined by `define_config!`.
///
/// Implemented on the generated reader type; `Dto` is the matching
/// serialization type. The remaining items return the schema's registry
/// data, including the (prefixed) data of any schemas mounted via `extern`.
pub trait ConfigSchema {
    type Dto: for<'a> Facet<'a> + Default + Clone + serde::Serialize + serde::de::DeserializeOwned;

    /// Defaulting rules for every key in the schema
    fn defaults(config_dir: &Path) -> Vec<FieldDefault>;

    /// Keys that can be read but not changed via normal config operations
    fn read_only_keys() -> Vec<Cow<'static, str>>;

    /// Deprecated key names and the canonical keys they map to
    fn aliases() -> Vec<DeprecatedKey>;

    /// Example values to show for keys in help/list output
    fn examples() -> Vec<KeyExamples>;

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

/// Remaps a mounted schema's keys into the mounting schema's key space.
pub fn prefix_keys(prefix: &str, keys: Vec<Cow<'static, str>>) -> Vec<Cow<'static, str>> {
    keys.into_iter().map(|k| prefix_key(prefix, k)).collect()
}

/// Remaps a mounted schema's deprecated key names into the mounting schema's
/// key space, on both the old and the canonical side.
pub fn prefix_aliases(prefix: &str, aliases: Vec<DeprecatedKey>) -> Vec<DeprecatedKey> {
    aliases
        .into_iter()
        .map(|a| DeprecatedKey {
            old: prefix_key(prefix, a.old),
            new: prefix_key(prefix, a.new),
        })
        .collect()
}

/// Remaps a mounted schema's example keys into the mounting schema's key space.
pub fn prefix_examples(prefix: &str, examples: Vec<KeyExamples>) -> Vec<KeyExamples> {
    examples
        .into_iter()
        .map(|(key, values)| (prefix_key(prefix, key), values))
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

    #[test]
    fn aliases_are_prefixed_on_both_sides() {
        let prefixed = prefix_aliases(
            "device",
            vec![DeprecatedKey {
                old: "identifier".into(),
                new: "id".into(),
            }],
        );
        assert_eq!(prefixed[0].old, "device.identifier");
        assert_eq!(prefixed[0].new, "device.id");
    }

    #[test]
    fn read_only_and_example_keys_are_prefixed() {
        assert_eq!(
            prefix_keys("device", vec!["id".into()]),
            vec![Cow::from("device.id")]
        );
        let examples = prefix_examples("device", vec![("id".into(), ["my-device"].as_slice())]);
        assert_eq!(examples[0].0, "device.id");
        assert_eq!(examples[0].1, ["my-device"]);
    }
}
