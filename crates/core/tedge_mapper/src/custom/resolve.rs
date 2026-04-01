//! Effective mapper configuration resolution.
//!
//! This module provides [`resolve_effective_config`], which applies all fallback
//! and inference logic to produce an [`EffectiveMapperConfig`] — the configuration
//! the mapper will actually use at runtime. The resolution logic is shared between
//! the mapper runtime and the `tedge mapper config get` / `tedge mapper list` CLI
//! commands so that both reflect exactly the same values.

use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use certificate::PemCertificate;
use tedge_config::cli::format_config_set_cmd;
use tedge_config::models::HostPort;
use tedge_config::models::SecondsOrHumanTime;
use tedge_config::models::MQTT_TLS_PORT;
use tedge_config::tedge_toml::Cloud;
use tedge_config::TEdgeConfig;
use tedge_flows::MapperParams;
use tedge_mqtt_bridge::config_toml::collect_string_array;
use tedge_mqtt_bridge::config_toml::toml_scalar_to_string;
use tedge_mqtt_bridge::config_toml::walk_toml_path;
use tedge_mqtt_bridge::config_toml::AuthMethod;
use tedge_mqtt_bridge::config_toml::MapperArrayResult;
use tedge_mqtt_bridge::config_toml::MapperConfigLookup;
use tedge_mqtt_bridge::config_toml::MapperKeyResult;
use tedge_mqtt_bridge::config_toml::WalkResult;

use crate::custom::config::AuthMethodConfig;
use crate::custom::config::BridgeConfig;
use crate::custom::config::BridgeTls;
use crate::custom::config::CustomMapperConfig;

/// Tracks the origin of a resolved configuration value.
#[derive(Debug, Clone)]
pub enum ConfigSource {
    /// Value was explicitly set in `mapper.toml`.
    MapperToml,
    /// Value was a relative path in `mapper.toml`; the stored value is the resolved
    /// absolute form. The original relative string is preserved for display.
    MapperTomlResolved { original: String },
    /// Value was not set in `mapper.toml` and was inherited from the root `tedge.toml`
    /// via a schema field (e.g. cert path, device id).
    TedgeToml,
    /// Value was not set in `mapper.toml` but is provided by the built-in mapper
    /// configuration (derived from `tedge.toml`). Users should change this via
    /// `tedge config set` rather than by editing `mapper.toml` directly.
    ///
    /// Only returned by [`EffectiveMapperConfig::get`] when an overlay was supplied
    /// to [`resolve_effective_config`] — custom mapper calls pass `None` so this
    /// variant is unreachable for custom mappers.
    TedgeConfig,
    /// Value was inferred from the Subject Common Name of the device certificate.
    CertificateCN { cert_path: Utf8PathBuf },
    /// Value is the schema default (not present in any configuration file).
    Default,
}

impl ConfigSource {
    /// Returns a short tag suitable for tabular output (e.g. `tedge mapper list`).
    pub fn short_tag(&self) -> &'static str {
        match self {
            ConfigSource::MapperToml | ConfigSource::MapperTomlResolved { .. } => "mapper.toml",
            ConfigSource::TedgeToml | ConfigSource::TedgeConfig => "tedge.toml",
            ConfigSource::CertificateCN { .. } => "cert CN",
            ConfigSource::Default => "default",
        }
    }
}

impl std::fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigSource::MapperToml => write!(f, "from mapper.toml"),
            ConfigSource::MapperTomlResolved { original } => write!(
                f,
                "relative path '{original}' in mapper.toml, resolved to absolute"
            ),
            ConfigSource::TedgeToml => {
                write!(f, "not set in mapper.toml, inherited from tedge.toml")
            }
            ConfigSource::TedgeConfig => {
                write!(f, "from tedge.toml")
            }
            ConfigSource::CertificateCN { cert_path } => {
                write!(f, "inferred from certificate CN ({cert_path})")
            }
            ConfigSource::Default => write!(f, "schema default"),
        }
    }
}

/// A resolved configuration value together with its origin.
#[derive(Debug, Clone)]
pub struct Sourced<T> {
    pub value: T,
    pub source: ConfigSource,
}

/// Result of [`EffectiveMapperConfig::get`].
///
/// Distinguishes three outcomes that `Option<T>` would collapse:
/// - [`Value`](Self::Value): the key is set and has an effective value.
/// - [`NotSet`](Self::NotSet): the key is a recognised schema field but has no configured value.
/// - [`UnknownKey`](Self::UnknownKey): the key is not in the schema and was not found in the
///   mapper's TOML table — it is likely a typo or an unsupported key.
#[derive(Debug)]
pub enum ConfigGetResult {
    /// Key found — includes the effective value and its origin.
    Value(Sourced<String>),
    /// Key is recognised (schema-level or in overlay schema) but has no configured value.
    NotSet,
    /// Key is not a schema field and was not present in the mapper's TOML table.
    UnknownKey,
}

#[cfg(test)]
impl ConfigGetResult {
    fn unwrap_value(self) -> Sourced<String> {
        match self {
            Self::Value(s) => s,
            other => panic!("called unwrap_value on {other:?}"),
        }
    }
}

/// The fully resolved effective configuration for a mapper instance.
///
/// Produced by [`resolve_effective_config`]. All path fields are absolute and all
/// fallbacks from the root `tedge.toml` have been applied.
///
/// `device_id` is `None` when cert auth is in use but the certificate cannot be
/// read — the mapper also cannot start in that state, so there is no honest value
/// to display.
#[derive(Debug)]
pub struct EffectiveMapperConfig {
    /// Contents of `mapper.toml` before any overlay is applied. Used by `get()` to
    /// distinguish keys the user set explicitly from those supplied by the built-in
    /// mapper configuration.
    mapper_table: toml::Table,
    /// Merged table: `mapper.toml` overlaid with the built-in mapper config (e.g. c8y).
    /// Private: callers use `to_template_table()` and `get()` instead.
    raw_table: toml::Table,
    /// Null-preserving JSON schema used by `get()` to distinguish [`ConfigGetResult::NotSet`]
    /// from [`ConfigGetResult::UnknownKey`] for keys not found in the TOML tables.
    ///
    /// For custom mappers this is built from [`CustomMapperConfig`] by
    /// [`build_custom_mapper_schema`]. For built-in mappers it is supplied by the caller
    /// (e.g. from `serde_json::to_value(c8y_reader)` so that `OptionalConfig::Empty` fields
    /// appear as `null` rather than being omitted as they would be in TOML serialisation).
    schema: serde_json::Value,
    /// Cloud broker URL (from `mapper.toml`), if configured.
    pub url: Option<Sourced<HostPort<MQTT_TLS_PORT>>>,
    /// Effective MQTT client ID. `None` when cert auth is in use but the cert is
    /// unreadable.
    pub device_id: Option<Sourced<String>>,
    /// TLS client certificate path (`mapper.toml` → `tedge.toml` fallback).
    pub cert_path: Option<Sourced<Utf8PathBuf>>,
    /// TLS client private key path (`mapper.toml` → `tedge.toml` fallback).
    pub key_path: Option<Sourced<Utf8PathBuf>>,
    /// CA certificate path for verifying the cloud broker (`mapper.toml` → `/etc/ssl/certs`).
    pub root_cert_path: Sourced<Utf8PathBuf>,
    /// Credentials file path for password authentication (`mapper.toml` only).
    pub credentials_path: Option<Sourced<Utf8PathBuf>>,
    /// Resolved effective authentication method (after `auto` expansion), with source.
    pub effective_auth: Sourced<AuthMethod>,
    /// MQTT bridge settings (keepalive interval, clean session).
    pub bridge: BridgeConfig,
    /// `true` when this config was built from a built-in mapper (an overlay was applied).
    /// Used to tailor error help text: built-in mapper keys should point to `tedge config set`,
    /// not `mapper.toml`.
    is_builtin: bool,
    /// The mapper name (e.g. `"c8y"` or `"c8y.prod"`), used to prefix keys in error messages
    /// and to generate `tedge config set` commands. Empty string when not set.
    mapper_name: String,
}

impl EffectiveMapperConfig {
    /// Returns a TOML table for `${mapper.*}` template expansion in bridge rules.
    ///
    /// Starts from the stored raw table (mapper.toml, possibly overlaid with built-in
    /// mapper config) so that non-schema keys such as `bridge.topic_prefix` remain
    /// accessible, then overlays the effective resolved values for schema-level keys
    /// (`device.id`, `device.cert_path`, etc.) so that cert CN inference and
    /// `tedge.toml` fallbacks are reflected in templates.
    ///
    /// The struct fields are the single source of truth: adding a new field to
    /// [`EffectiveMapperConfig`] and wiring it in here is sufficient — no separate
    /// list of manual inserts to keep in sync elsewhere.
    pub fn to_template_table(&self) -> toml::Table {
        let mut table = self.raw_table.clone();

        let device = table
            .entry("device".to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));

        if let toml::Value::Table(dt) = device {
            if let Some(id) = &self.device_id {
                dt.insert("id".to_string(), toml::Value::String(id.value.clone()));
            }
            if let Some(cert) = &self.cert_path {
                dt.insert(
                    "cert_path".to_string(),
                    toml::Value::String(cert.value.to_string()),
                );
            }
            if let Some(key) = &self.key_path {
                dt.insert(
                    "key_path".to_string(),
                    toml::Value::String(key.value.to_string()),
                );
            }
            dt.insert(
                "root_cert_path".to_string(),
                toml::Value::String(self.root_cert_path.value.to_string()),
            );
        }

        if let Some(u) = &self.url {
            table.insert("url".to_string(), toml::Value::String(u.value.to_string()));
        }

        table
    }

    /// Returns the effective value of a config key, with source annotation.
    ///
    /// Schema-level keys (`url`, `device.id`, `device.cert_path`, `device.key_path`,
    /// `device.root_cert_path`, `credentials_path`, `auth_method`) are resolved first
    /// and return the effective value with full source tracking. All other keys fall back
    /// to walking the raw TOML table. If a key is not found in any TOML table, the stored
    /// schema JSON is consulted to distinguish [`ConfigGetResult::NotSet`] (key is
    /// schema-valid but has no configured value) from [`ConfigGetResult::UnknownKey`].
    pub fn get(&self, key: &str) -> ConfigGetResult {
        match key {
            "url" => match &self.url {
                Some(s) => ConfigGetResult::Value(Sourced {
                    value: s.value.to_string(),
                    source: s.source.clone(),
                }),
                None => self.get_from_tables(key),
            },
            "device.id" => match &self.device_id {
                Some(s) => ConfigGetResult::Value(Sourced {
                    value: s.value.clone(),
                    source: s.source.clone(),
                }),
                None => self.get_from_tables(key),
            },
            "device.cert_path" => match &self.cert_path {
                Some(s) => ConfigGetResult::Value(Sourced {
                    value: s.value.to_string(),
                    source: s.source.clone(),
                }),
                None => self.get_from_tables(key),
            },
            "device.key_path" => match &self.key_path {
                Some(s) => ConfigGetResult::Value(Sourced {
                    value: s.value.to_string(),
                    source: s.source.clone(),
                }),
                None => self.get_from_tables(key),
            },
            "device.root_cert_path" => ConfigGetResult::Value(Sourced {
                value: self.root_cert_path.value.to_string(),
                source: self.root_cert_path.source.clone(),
            }),
            "credentials_path" => match &self.credentials_path {
                Some(s) => ConfigGetResult::Value(Sourced {
                    value: s.value.to_string(),
                    source: s.source.clone(),
                }),
                None => self.get_from_tables(key),
            },
            "auth_method" => ConfigGetResult::Value(Sourced {
                value: self.effective_auth.value.to_string(),
                source: self.effective_auth.source.clone(),
            }),
            "bridge.tls.enable" => {
                let (value, source) =
                    match walk_table_value(&self.mapper_table, ["bridge", "tls", "enable"]) {
                        Some(_) => (self.bridge.tls.enable, ConfigSource::MapperToml),
                        None => (self.bridge.tls.enable, ConfigSource::Default),
                    };
                ConfigGetResult::Value(Sourced {
                    value: value.to_string(),
                    source,
                })
            }
            _ => self.get_from_tables(key),
        }
    }

    /// Looks up a key in the TOML tables, falling back to the schema to
    /// distinguish `NotSet` from `UnknownKey`.
    ///
    /// Used both by the catch-all branch of [`get`] and as the fallback when
    /// a typed schema-level field is `None` (the value may still be present
    /// in the overlay from `tedge.toml`).
    fn get_from_tables(&self, key: &str) -> ConfigGetResult {
        // Check mapper.toml first — user-set values take priority and are
        // attributed to mapper.toml. If only found in raw_table (i.e. it came
        // from the overlay), attribute it to TedgeConfig instead. This branch
        // is only reachable when an overlay was supplied (i.e. a built-in mapper)
        // see ConfigSource::TedgeConfig.
        if let Some(v) = walk_table_value(&self.mapper_table, key.split('.')) {
            return ConfigGetResult::Value(Sourced {
                value: toml_value_display(v),
                source: ConfigSource::MapperToml,
            });
        }
        if let Some(v) = walk_table_value(&self.raw_table, key.split('.')) {
            return ConfigGetResult::Value(Sourced {
                value: toml_value_display(v),
                source: ConfigSource::TedgeConfig,
            });
        }
        // Key not found in any TOML table — use the schema JSON to determine
        // whether the key is a known field (NotSet) or entirely unrecognised
        // (UnknownKey).
        match walk_json_value(&self.schema, key.split('.')) {
            Some(_) => ConfigGetResult::NotSet,
            None => ConfigGetResult::UnknownKey,
        }
    }

    /// Returns a `with_mapper_name` builder that sets the mapper name used
    /// to prefix keys in error messages and generate `tedge config set` commands.
    /// Only meaningful for built-in mappers (e.g. `"c8y"`, `"c8y.prod"`).
    pub fn with_mapper_name(mut self, name: impl Into<String>) -> Self {
        self.mapper_name = name.into();
        self
    }

    fn not_set_help(&self, path: &str) -> String {
        if self.is_builtin {
            let cmd = if self.mapper_name.is_empty() {
                format!("tedge config set <cloud>.{path} <value>")
            } else {
                format_config_set_cmd(&self.mapper_name, path)
            };
            format!("Configure this via '{cmd}'")
        } else {
            format!("Set '{path}' in mapper.toml")
        }
    }

    /// Returns the display key for error messages.
    /// For built-in mappers this prefixes with `<mapper_name>.` (e.g. `"c8y.url"`)
    /// since that is how the key is addressed via `tedge config set`.
    /// For custom mappers the plain path is used — the key lives in `mapper.toml`
    /// without any namespace prefix.
    fn display_key(&self, path: &str) -> Option<String> {
        if self.is_builtin && !self.mapper_name.is_empty() {
            Some(format!("{}.{path}", self.mapper_name))
        } else {
            None
        }
    }

    /// Classifies a missing key as `NotSet` or `UnknownKey`.
    ///
    /// Schema-level keys (those matched by `is_schema_key`) are always `NotSet`;
    /// other keys consult the JSON schema to decide.
    fn not_found_scalar(&self, path: &str, is_schema_key: bool) -> MapperKeyResult {
        if is_schema_key {
            MapperKeyResult::NotSet {
                help: Some(self.not_set_help(path)),
                display_key: self.display_key(path),
            }
        } else {
            match walk_json_value(&self.schema, path.split('.')) {
                Some(_) => MapperKeyResult::NotSet {
                    help: Some(self.not_set_help(path)),
                    display_key: self.display_key(path),
                },
                None => MapperKeyResult::UnknownKey,
            }
        }
    }
}

impl MapperConfigLookup for EffectiveMapperConfig {
    fn lookup_scalar(&self, path: &str) -> MapperKeyResult {
        // Schema-level keys: first try the effective typed value (cert CN inference,
        // tedge.toml fallbacks). If the typed field is not set, fall through to raw_table
        // so that an explicit mapper.toml value (e.g. `device.id`) is still usable even
        // when the typed resolution path is unavailable (e.g. cert not readable yet).
        //
        // N.B. When adding a new typed field to `EffectiveMapperConfig`, remember to add
        // the corresponding dotted path here so it participates in the typed resolution
        // path. Otherwise the field will only be reachable via the raw TOML fallback.
        let is_schema_key = matches!(
            path,
            "url"
                | "device.id"
                | "device.cert_path"
                | "device.key_path"
                | "device.root_cert_path"
                | "credentials_path"
                | "auth_method"
                | "bridge.tls"
        );
        if is_schema_key {
            if let ConfigGetResult::Value(s) = self.get(path) {
                return MapperKeyResult::Value(s.value);
            }
        }

        // Walk the raw table (mapper.toml + overlay) with scalar type checks.
        match walk_toml_path(&self.raw_table, path) {
            WalkResult::Found(v) => match toml_scalar_to_string(v) {
                Some(s) => MapperKeyResult::Value(s),
                None => MapperKeyResult::NotScalar,
            },
            WalkResult::BadIntermediatePath { intermediate } => {
                MapperKeyResult::BadIntermediatePath { intermediate }
            }
            WalkResult::NotFound => self.not_found_scalar(path, is_schema_key),
        }
    }

    fn lookup_array(&self, path: &str) -> MapperArrayResult {
        match walk_toml_path(&self.raw_table, path) {
            WalkResult::Found(v) => collect_string_array(v),
            WalkResult::BadIntermediatePath { intermediate } => {
                MapperArrayResult::BadIntermediatePath { intermediate }
            }
            WalkResult::NotFound => match walk_json_value(&self.schema, path.split('.')) {
                Some(_) => MapperArrayResult::NotSet {
                    help: Some(self.not_set_help(path)),
                    display_key: self.display_key(path),
                },
                None => MapperArrayResult::UnknownKey,
            },
        }
    }
}

impl MapperParams for EffectiveMapperConfig {
    fn get_value(&self, key: &str) -> Option<String> {
        match self.get(key) {
            ConfigGetResult::Value(s) => Some(s.value),
            _ => None,
        }
    }
}

/// Mirror of [`CustomMapperConfig`] used solely to produce a null-preserving JSON schema.
///
/// All optional fields serialise to `null` (via `Option<T>`) rather than being omitted,
/// so callers can distinguish [`ConfigGetResult::NotSet`] from [`ConfigGetResult::UnknownKey`].
/// The `device` section is always present (never `null` at the top level) so that sub-keys
/// such as `device.cert_path` are always reachable in the serialised schema.
#[derive(serde::Serialize)]
struct CustomMapperSchema<'a> {
    url: Option<&'a HostPort<MQTT_TLS_PORT>>,
    device: DeviceSchema<'a>,
    bridge: BridgeSchema<'a>,
    auth_method: AuthMethodConfig,
    credentials_path: Option<&'a Utf8PathBuf>,
}

#[derive(serde::Serialize)]
struct DeviceSchema<'a> {
    id: Option<&'a str>,
    cert_path: Option<&'a Utf8Path>,
    key_path: Option<&'a Utf8Path>,
    root_cert_path: Option<&'a Utf8Path>,
}

#[derive(serde::Serialize)]
struct BridgeSchema<'a> {
    clean_session: bool,
    keepalive_interval: Option<&'a SecondsOrHumanTime>,
    tls: BridgeTls,
}

/// Returns all known schema-level key paths for a custom mapper config (e.g. `"device.cert_path"`).
///
/// Keys are derived from the [`CustomMapperSchema`] struct by serialising an empty schema and
/// collecting all dotted leaf paths, so this stays in sync with the schema automatically.
pub fn custom_mapper_schema_keys() -> Vec<String> {
    let schema = build_custom_mapper_schema(&CustomMapperConfig {
        table: toml::Table::new(),
        cloud_type: None,
        url: None,
        device: None,
        bridge: BridgeConfig::default(),
        auth_method: AuthMethodConfig::Auto,
        credentials_path: None,
    });
    collect_schema_leaf_keys(&schema, String::new())
}

fn collect_schema_leaf_keys(value: &serde_json::Value, prefix: String) -> Vec<String> {
    if let serde_json::Value::Object(map) = value {
        let mut keys = Vec::new();
        for (k, v) in map {
            let path = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            if matches!(v, serde_json::Value::Object(_)) {
                keys.extend(collect_schema_leaf_keys(v, path));
            } else {
                keys.push(path);
            }
        }
        keys
    } else {
        vec![prefix]
    }
}

/// Builds a null-preserving JSON schema from a [`CustomMapperConfig`].
///
/// Every field in the mapper schema is present in the returned value: optional fields that
/// are not configured appear as `null`. The `device` section is always expanded to its
/// sub-fields so that keys like `device.cert_path` are reachable even when no `[device]`
/// section was set in `mapper.toml`.
fn build_custom_mapper_schema(config: &CustomMapperConfig) -> serde_json::Value {
    let d = config.device.as_ref();
    serde_json::to_value(CustomMapperSchema {
        url: config.url.as_ref(),
        device: DeviceSchema {
            id: d.and_then(|d| d.id.as_deref()),
            cert_path: d.and_then(|d| d.cert_path.as_deref()),
            key_path: d.and_then(|d| d.key_path.as_deref()),
            root_cert_path: d.and_then(|d| d.root_cert_path.as_deref()),
        },
        bridge: BridgeSchema {
            clean_session: config.bridge.clean_session,
            keepalive_interval: config.bridge.keepalive_interval.as_ref(),
            tls: config.bridge.tls,
        },
        auth_method: config.auth_method,
        credentials_path: config.credentials_path.as_ref(),
    })
    .expect("schema serialisation is infallible")
}

/// Converts a cloud-specific config reader directly to a [`toml::Table`] without
/// going through a string intermediate.
pub fn reader_to_toml_table(reader: &impl serde::Serialize) -> anyhow::Result<toml::Table> {
    let value =
        toml::Value::try_from(reader).context("failed to serialise config reader to TOML value")?;
    match value {
        toml::Value::Table(t) => Ok(t),
        other => anyhow::bail!(
            "expected config reader to serialise as a TOML table, got {}",
            other.type_str()
        ),
    }
}

/// Resolves a [`CustomMapperConfig`] into an [`EffectiveMapperConfig`].
///
/// Resolution order per field:
/// - `cert_path` / `key_path`: `mapper.toml` → root `tedge.toml`
/// - `root_cert_path`: `mapper.toml` → `/etc/ssl/certs` (default)
/// - `device_id` (cert auth): cert CN → explicit `device.id` → root `tedge.toml`
/// - `device_id` (cert auth, unreadable cert): `None` (no fallback — avoids misleading output)
/// - `device_id` (password auth): explicit `device.id` → root `tedge.toml`
///
/// Missing path values are returned as `None`; callers are responsible for failing
/// if a required field is absent.
///
/// `schema_override` supplies a null-preserving JSON schema for [`EffectiveMapperConfig::get`].
/// Built-in mappers (e.g. c8y) should pass `serde_json::to_value(reader)?` so that
/// `OptionalConfig::Empty` fields appear as `null` in the schema rather than being omitted as
/// they would be under TOML serialisation. When `None`, the schema is derived from `config`
/// via [`build_custom_mapper_schema`].
pub async fn resolve_effective_config(
    config: &CustomMapperConfig,
    tedge_config: &TEdgeConfig,
    overlay: Option<&toml::Table>,
    schema_override: Option<serde_json::Value>,
) -> anyhow::Result<EffectiveMapperConfig> {
    let effective_auth = match config.auth_method {
        AuthMethodConfig::Certificate => Sourced {
            value: AuthMethod::Certificate,
            source: ConfigSource::MapperToml,
        },
        AuthMethodConfig::Password => Sourced {
            value: AuthMethod::Password,
            source: ConfigSource::MapperToml,
        },
        AuthMethodConfig::Auto => Sourced {
            value: if config.credentials_path.is_some() {
                AuthMethod::Password
            } else {
                AuthMethod::Certificate
            },
            source: ConfigSource::Default,
        },
    };

    let url = config.url.clone().map(|u| Sourced {
        value: u,
        source: ConfigSource::MapperToml,
    });

    let cert_path = resolve_path_sourced(
        config.device.as_ref().and_then(|d| d.cert_path.as_ref()),
        &config.table,
        &["device", "cert_path"],
        || {
            tedge_config
                .device_cert_path(None::<tedge_config::tedge_toml::tedge_config::Cloud<'_>>)
                .ok()
                .map(|p| p.into())
        },
    );

    let key_path = resolve_path_sourced(
        config.device.as_ref().and_then(|d| d.key_path.as_ref()),
        &config.table,
        &["device", "key_path"],
        || {
            tedge_config
                .device_key_path(None::<tedge_config::tedge_toml::tedge_config::Cloud<'_>>)
                .ok()
                .map(|p| p.into())
        },
    );

    let root_cert_path = resolve_path_sourced(
        config
            .device
            .as_ref()
            .and_then(|d| d.root_cert_path.as_ref()),
        &config.table,
        &["device", "root_cert_path"],
        || None,
    )
    .unwrap_or_else(|| Sourced {
        value: Utf8PathBuf::from("/etc/ssl/certs"),
        source: ConfigSource::Default,
    });

    let credentials_path = config.credentials_path.clone().map(|p| Sourced {
        value: p,
        source: ConfigSource::MapperToml,
    });

    let device_id = resolve_device_id(
        config,
        tedge_config,
        &effective_auth.value,
        cert_path.as_ref(),
    )
    .await;

    let mapper_table = config.table.clone();
    let mut raw_table = config.table.clone();
    let is_builtin = overlay.is_some();
    if let Some(ov) = overlay {
        deep_merge(&mut raw_table, ov);
    }

    let schema = schema_override.unwrap_or_else(|| build_custom_mapper_schema(config));

    Ok(EffectiveMapperConfig {
        mapper_table,
        raw_table,
        schema,
        url,
        device_id,
        cert_path,
        key_path,
        root_cert_path,
        credentials_path,
        effective_auth,
        bridge: config.bridge.clone(),
        is_builtin,
        mapper_name: String::new(),
    })
}

/// Recursively merges `overlay` onto `base`. Overlay values win on conflict; for
/// tables, the merge descends recursively so non-conflicting keys are preserved.
fn deep_merge(base: &mut toml::Table, overlay: &toml::Table) {
    for (key, value) in overlay {
        match (base.get_mut(key), value) {
            (Some(toml::Value::Table(base_t)), toml::Value::Table(overlay_t)) => {
                deep_merge(base_t, overlay_t);
            }
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Walks a nested JSON object and returns the value at the given key path.
///
/// Returns `None` if any segment is absent. Returning `Some(serde_json::Value::Null)` means
/// the path exists in the schema but the value is not configured.
fn walk_json_value(
    json: &serde_json::Value,
    keys: impl IntoIterator<Item: AsRef<str>>,
) -> Option<&serde_json::Value> {
    let mut current = json;
    let mut iter = keys.into_iter().peekable();
    loop {
        let key = iter.next()?;
        let key = key.as_ref();
        let obj = current.as_object()?;
        current = obj.get(key)?;
        if iter.peek().is_none() {
            return Some(current);
        }
    }
}

/// Walks a nested TOML table and returns the value at the given key path.
fn walk_table_value(
    table: &toml::Table,
    keys: impl IntoIterator<Item: AsRef<str>>,
) -> Option<&toml::Value> {
    let mut current = table;
    let mut iter = keys.into_iter().peekable();
    loop {
        let key = iter.next()?;
        let key = key.as_ref();
        if iter.peek().is_none() {
            return current.get(key);
        }
        current = current.get(key)?.as_table()?;
    }
}

/// Renders a TOML value as a plain string for display.
fn toml_value_display(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Datetime(d) => d.to_string(),
        toml::Value::Array(_) | toml::Value::Table(_) => value.to_string(),
    }
}

/// Resolves a path field, preserving information about whether it was a relative
/// path in `mapper.toml`. Uses the already-resolved (absolute) path from the
/// parsed `CustomMapperConfig` and checks the raw TOML table to determine whether
/// the original was relative.
fn resolve_path_sourced(
    resolved: Option<&Utf8PathBuf>,
    table: &toml::Table,
    table_key_path: &[&str],
    tedge_fallback: impl FnOnce() -> Option<Utf8PathBuf>,
) -> Option<Sourced<Utf8PathBuf>> {
    match resolved {
        Some(path) => {
            let original_str = walk_table_value(table, table_key_path).and_then(|v| v.as_str());
            let source = match original_str {
                Some(s) if !Utf8Path::new(s).is_absolute() => ConfigSource::MapperTomlResolved {
                    original: s.to_string(),
                },
                _ => ConfigSource::MapperToml,
            };
            Some(Sourced {
                value: path.clone(),
                source,
            })
        }
        None => tedge_fallback().map(|p| Sourced {
            value: p,
            source: ConfigSource::TedgeToml,
        }),
    }
}

/// Resolves the effective MQTT client ID with source annotation.
///
/// For certificate auth:
/// - Cert readable with non-empty CN → use CN (source: `CertificateCN`)
/// - Cert readable but no CN → fall through to explicit `device.id` or `tedge.toml`
/// - Cert unreadable → `None` (mapper will also fail at runtime; don't show a
///   misleading value)
///
/// For password auth: explicit `device.id` → `tedge.toml` device_id → `None`
async fn resolve_device_id(
    config: &CustomMapperConfig,
    tedge_config: &TEdgeConfig,
    effective_auth: &AuthMethod,
    cert_path: Option<&Sourced<Utf8PathBuf>>,
) -> Option<Sourced<String>> {
    let explicit_id = config.device.as_ref().and_then(|d| d.id.clone());
    let tedge_id = tedge_config
        .device_id(None::<Cloud<'_>>)
        .ok()
        .filter(|id| !id.is_empty());

    match effective_auth {
        AuthMethod::Certificate => {
            let cert = cert_path?.value.clone();
            match PemCertificate::from_pem_file(&cert).and_then(|c| c.subject_common_name()) {
                Ok(cn) if !cn.is_empty() => Some(Sourced {
                    value: cn,
                    source: ConfigSource::CertificateCN {
                        cert_path: cert.clone(),
                    },
                }),
                Ok(_) => {
                    // Cert readable but no CN — fall through to explicit id / tedge.toml
                    explicit_id
                        .map(|id| Sourced {
                            value: id,
                            source: ConfigSource::MapperToml,
                        })
                        .or_else(|| {
                            tedge_id.map(|id| Sourced {
                                value: id,
                                source: ConfigSource::TedgeToml,
                            })
                        })
                }
                Err(_) => {
                    // Cert unreadable — return None so callers don't show a misleading
                    // value. The mapper cannot start without a readable cert either.
                    None
                }
            }
        }
        AuthMethod::Password => explicit_id
            .map(|id| Sourced {
                value: id,
                source: ConfigSource::MapperToml,
            })
            .or_else(|| {
                tedge_id.map(|id| Sourced {
                    value: id,
                    source: ConfigSource::TedgeToml,
                })
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::config::load_mapper_config;
    use crate::custom::config::BridgeConfig;
    use crate::custom::config::DeviceConfig;
    use camino::Utf8PathBuf;
    use tedge_config::TEdgeConfig;
    use tedge_test_utils::fs::TempTedgeDir;

    // Test EC certificate (CN = "localhost") and matching private key.
    // Same constants used in mapper.rs tests.
    const TEST_CERT_PEM: &str = "\
-----BEGIN CERTIFICATE-----\n\
MIIBnzCCAUWgAwIBAgIUSTUtJUfUdERMKBwsfdRv9IbvQicwCgYIKoZIzj0EAwIw\n\
FDESMBAGA1UEAwwJbG9jYWxob3N0MCAXDTIzMTExNDE2MDUwOVoYDzMwMjMwMzE3\n\
MTYwNTA5WjAUMRIwEAYDVQQDDAlsb2NhbGhvc3QwWTATBgcqhkjOPQIBBggqhkjO\n\
PQMBBwNCAAR2SVEPD34AAxFuk0xYm60p7hA7+1SW+sFHazBRg32ifFd0o2Mn+Tf+\n\
voYflBi3v4lhr361RoWB8QfmaGN05vv+o3MwcTAdBgNVHQ4EFgQUAb4jQ7RQ/xyg\n\
cZM+We8ik29/oxswHwYDVR0jBBgwFoAUAb4jQ7RQ/xygcZM+We8ik29/oxswIQYD\n\
VR0RBBowGIIJbG9jYWxob3N0ggsqLmxvY2FsaG9zdDAMBgNVHRMBAf8EAjAAMAoG\n\
CCqGSM49BAMCA0gAMEUCIA6QrxoDHQJqoly7d8VN0sj0eDvfFpbbZdSnzBd6R8AP\n\
AiEAm/PAH3IPGuHRBIpdC0rNR8F/l3WcN9I9984qKZdG5rs=\n\
-----END CERTIFICATE-----\n";

    const TEST_KEY_PEM: &str = "\
-----BEGIN EC PRIVATE KEY-----\n\
MHcCAQEEIBX2Z/NKGEX14QbH4kb5GXom0pqSPfX0mxdWbLb86apEoAoGCCqGSM49\n\
AwEHoUQDQgAEdklRDw9+AAMRbpNMWJutKe4QO/tUlvrBR2swUYN9onxXdKNjJ/k3\n\
/r6GH5QYt7+JYa9+tUaFgfEH5mhjdOb7/g==\n\
-----END EC PRIVATE KEY-----\n";

    async fn write_cert(dir: &Utf8Path) -> (Utf8PathBuf, Utf8PathBuf) {
        let cert = dir.join("cert.pem");
        let key = dir.join("key.pem");
        tokio::fs::write(&cert, TEST_CERT_PEM).await.unwrap();
        tokio::fs::write(&key, TEST_KEY_PEM).await.unwrap();
        (cert, key)
    }

    async fn write_cert_no_cn(dir: &Utf8Path) -> (Utf8PathBuf, Utf8PathBuf) {
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let mut params = rcgen::CertificateParams::default();
        params.distinguished_name = rcgen::DistinguishedName::new();
        let issuer = rcgen::Issuer::from_params(&params, &key);
        let cert = params.signed_by(&key, &issuer).unwrap();
        let cert_path = dir.join("cert-no-cn.pem");
        let key_path = dir.join("key-no-cn.pem");
        tokio::fs::write(&cert_path, cert.pem()).await.unwrap();
        tokio::fs::write(&key_path, key.serialize_pem())
            .await
            .unwrap();
        (cert_path, key_path)
    }

    fn make_config(url: Option<&str>) -> CustomMapperConfig {
        CustomMapperConfig {
            table: toml::Table::new(),
            cloud_type: None,
            url: url.map(|u| u.parse().unwrap()),
            device: None,
            bridge: BridgeConfig::default(),
            auth_method: AuthMethodConfig::Auto,
            credentials_path: None,
        }
    }

    #[tokio::test]
    async fn cert_cn_used_as_device_id() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let (cert, key) = write_cert(ttd.utf8_path()).await;
        let tedge_config = TEdgeConfig::load_toml_str(&format!(
            "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
        ));
        let config = make_config(Some("mqtt.example.com:1883"));

        let effective = resolve_effective_config(&config, &tedge_config, None, None)
            .await
            .unwrap();

        let id = effective.device_id.unwrap();
        assert_eq!(id.value, "localhost");
        assert!(matches!(id.source, ConfigSource::CertificateCN { .. }));
    }

    #[tokio::test]
    async fn explicit_device_id_overridden_by_cert_cn() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let (cert, key) = write_cert(ttd.utf8_path()).await;
        let tedge_config = TEdgeConfig::load_toml_str(&format!(
            "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
        ));
        let mut config = make_config(Some("mqtt.example.com:1883"));
        config.device = Some(DeviceConfig {
            id: Some("explicit-id".to_string()),
            cert_path: None,
            key_path: None,
            root_cert_path: None,
        });

        let effective = resolve_effective_config(&config, &tedge_config, None, None)
            .await
            .unwrap();

        // CN takes precedence over explicit id
        assert_eq!(effective.device_id.unwrap().value, "localhost");
    }

    #[tokio::test]
    async fn cert_with_no_cn_falls_back_to_explicit_device_id() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let (cert, key) = write_cert_no_cn(ttd.utf8_path()).await;
        let tedge_config = TEdgeConfig::load_toml_str(&format!(
            "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
        ));
        let mut config = make_config(Some("mqtt.example.com:1883"));
        config.device = Some(DeviceConfig {
            id: Some("fallback-device".to_string()),
            cert_path: None,
            key_path: None,
            root_cert_path: None,
        });

        let effective = resolve_effective_config(&config, &tedge_config, None, None)
            .await
            .unwrap();

        let id = effective.device_id.unwrap();
        assert_eq!(id.value, "fallback-device");
        assert!(matches!(id.source, ConfigSource::MapperToml));
    }

    #[tokio::test]
    async fn unreadable_cert_yields_none_device_id() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let tedge_config = TEdgeConfig::load_toml_str(
            "device.cert_path = \"/nonexistent/cert.pem\"\n\
             device.key_path = \"/nonexistent/key.pem\"\n",
        );
        let config = make_config(Some("mqtt.example.com:1883"));

        let effective = resolve_effective_config(&config, &tedge_config, None, None)
            .await
            .unwrap();

        assert!(
            effective.device_id.is_none(),
            "device_id should be None when cert is unreadable"
        );
    }

    #[tokio::test]
    async fn password_auth_uses_explicit_device_id() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let creds_path = ttd.utf8_path().join("creds.toml");
        tokio::fs::write(
            &creds_path,
            "[credentials]\nusername = \"u\"\npassword = \"p\"\n",
        )
        .await
        .unwrap();
        let tedge_config = TEdgeConfig::load_toml_str("");
        let mut config = make_config(Some("mqtt.example.com:1883"));
        config.credentials_path = Some(creds_path);
        config.device = Some(DeviceConfig {
            id: Some("my-device".to_string()),
            cert_path: None,
            key_path: None,
            root_cert_path: None,
        });

        let effective = resolve_effective_config(&config, &tedge_config, None, None)
            .await
            .unwrap();

        let id = effective.device_id.unwrap();
        assert_eq!(id.value, "my-device");
        assert!(matches!(id.source, ConfigSource::MapperToml));
    }

    #[tokio::test]
    async fn password_auth_falls_back_to_tedge_device_id() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let creds_path = ttd.utf8_path().join("creds.toml");
        tokio::fs::write(
            &creds_path,
            "[credentials]\nusername = \"u\"\npassword = \"p\"\n",
        )
        .await
        .unwrap();
        let tedge_config = TEdgeConfig::load_toml_str("device.id = \"root-device\"");
        let mut config = make_config(Some("mqtt.example.com:1883"));
        config.credentials_path = Some(creds_path);

        let effective = resolve_effective_config(&config, &tedge_config, None, None)
            .await
            .unwrap();

        let id = effective.device_id.unwrap();
        assert_eq!(id.value, "root-device");
        assert!(matches!(id.source, ConfigSource::TedgeToml));
    }

    #[tokio::test]
    async fn cert_path_from_mapper_toml_is_sourced_correctly() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let (cert, key) = write_cert(ttd.utf8_path()).await;
        let toml = format!("[device]\ncert_path = \"{cert}\"\nkey_path = \"{key}\"\n");
        tokio::fs::write(mapper_dir.join("mapper.toml"), &toml)
            .await
            .unwrap();
        let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
        let tedge_config = TEdgeConfig::load_toml_str("");

        let effective = resolve_effective_config(&config, &tedge_config, None, None)
            .await
            .unwrap();

        assert!(matches!(
            effective.cert_path.unwrap().source,
            ConfigSource::MapperToml
        ));
    }

    #[tokio::test]
    async fn relative_cert_path_annotated_as_resolved() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        // write actual cert files so load_mapper_config doesn't fail on validation
        let (_, _) = write_cert(&mapper_dir).await;
        tokio::fs::write(
            mapper_dir.join("mapper.toml"),
            "[device]\ncert_path = \"cert.pem\"\nkey_path = \"key.pem\"\n",
        )
        .await
        .unwrap();
        let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
        let tedge_config = TEdgeConfig::load_toml_str("");

        let effective = resolve_effective_config(&config, &tedge_config, None, None)
            .await
            .unwrap();

        let cert = effective.cert_path.unwrap();
        // Absolute path returned
        assert!(cert.value.is_absolute());
        // Source annotated as relative→resolved
        assert!(
            matches!(cert.source, ConfigSource::MapperTomlResolved { ref original } if original == "cert.pem"),
            "expected MapperTomlResolved with original='cert.pem', got {:?}",
            cert.source
        );
    }

    #[tokio::test]
    async fn cert_path_falls_back_to_tedge_toml() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let (cert, key) = write_cert(ttd.utf8_path()).await;
        let tedge_config = TEdgeConfig::load_toml_str(&format!(
            "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
        ));
        // No cert_path in mapper.toml
        let config = make_config(Some("mqtt.example.com:1883"));

        let effective = resolve_effective_config(&config, &tedge_config, None, None)
            .await
            .unwrap();

        assert!(matches!(
            effective.cert_path.unwrap().source,
            ConfigSource::TedgeToml
        ));
    }

    #[tokio::test]
    async fn root_cert_path_defaults_to_etc_ssl_certs() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let tedge_config = TEdgeConfig::load_toml_str("");
        let config = make_config(None);

        let effective = resolve_effective_config(&config, &tedge_config, None, None)
            .await
            .unwrap();

        assert_eq!(
            effective.root_cert_path.value,
            Utf8PathBuf::from("/etc/ssl/certs")
        );
        assert!(matches!(
            effective.root_cert_path.source,
            ConfigSource::Default
        ));
    }

    mod deep_merge_fn {
        use super::*;

        fn parse(toml: &str) -> toml::Table {
            toml.parse().unwrap()
        }

        #[test]
        fn overlay_wins_on_conflict() {
            let mut base = parse("url = \"base.example.com\"\n");
            let overlay = parse("url = \"overlay.example.com\"\n");
            deep_merge(&mut base, &overlay);
            assert_eq!(base["url"].as_str().unwrap(), "overlay.example.com");
        }

        #[test]
        fn non_conflicting_keys_preserved() {
            let mut base = parse("a = \"from-base\"\n");
            let overlay = parse("b = \"from-overlay\"\n");
            deep_merge(&mut base, &overlay);
            assert_eq!(base["a"].as_str().unwrap(), "from-base");
            assert_eq!(base["b"].as_str().unwrap(), "from-overlay");
        }

        #[test]
        fn table_merge_is_recursive() {
            let mut base = parse("[device]\nid = \"base-id\"\ncert_path = \"/base/cert.pem\"\n");
            let overlay = parse("[device]\nid = \"overlay-id\"\n");
            deep_merge(&mut base, &overlay);
            let device = base["device"].as_table().unwrap();
            // overlay wins on the conflicting key
            assert_eq!(device["id"].as_str().unwrap(), "overlay-id");
            // non-conflicting key from base is preserved
            assert_eq!(device["cert_path"].as_str().unwrap(), "/base/cert.pem");
        }
    }

    mod get_method {
        use super::*;

        #[tokio::test]
        async fn schema_key_device_id_from_cert_cn() {
            let ttd = TempTedgeDir::new();
            let (cert, key) = write_cert(ttd.utf8_path()).await;
            let tedge_config = TEdgeConfig::load_toml_str(&format!(
                "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
            ));
            let config = make_config(Some("mqtt.example.com:1883"));

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            let sourced = effective.get("device.id").unwrap_value();
            assert_eq!(sourced.value, "localhost");
            assert!(matches!(sourced.source, ConfigSource::CertificateCN { .. }));
        }

        #[tokio::test]
        async fn schema_key_cert_path_from_tedge_toml_fallback() {
            let ttd = TempTedgeDir::new();
            let (cert, key) = write_cert(ttd.utf8_path()).await;
            let tedge_config = TEdgeConfig::load_toml_str(&format!(
                "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
            ));
            // mapper.toml has no cert_path → falls back to tedge.toml
            let config = make_config(Some("mqtt.example.com:1883"));

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            let sourced = effective.get("device.cert_path").unwrap_value();
            assert_eq!(sourced.value, cert.to_string());
            assert!(matches!(sourced.source, ConfigSource::TedgeToml));
        }

        #[tokio::test]
        async fn non_schema_key_from_raw_table() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let mut config = make_config(None);
            config.table = "[bridge]\ntopic_prefix = \"c8y\"\n".parse().unwrap();

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            let sourced = effective.get("bridge.topic_prefix").unwrap_value();
            assert_eq!(sourced.value, "c8y");
            assert!(matches!(sourced.source, ConfigSource::MapperToml));
        }

        #[tokio::test]
        async fn non_schema_key_from_overlay_is_attributed_to_tedge_config() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let overlay: toml::Table = "[smartrest]\ntemplates = [\"12345\"]\n".parse().unwrap();

            let effective = resolve_effective_config(&config, &tedge_config, Some(&overlay), None)
                .await
                .unwrap();

            let sourced = effective.get("smartrest.templates").unwrap_value();
            assert_eq!(sourced.value, "[\"12345\"]");
            // Key came from the overlay (built-in mapper config), not mapper.toml
            assert!(matches!(sourced.source, ConfigSource::TedgeConfig));
        }

        #[tokio::test]
        async fn mapper_toml_key_shadows_overlay_key() {
            // If the same key is set in both mapper.toml and the overlay, the
            // mapper.toml value wins and is attributed to MapperToml.
            let tedge_config = TEdgeConfig::load_toml_str("");
            let mut config = make_config(None);
            config.table = "[smartrest]\ntemplates = [\"local\"]\n".parse().unwrap();
            let overlay: toml::Table = "[smartrest]\ntemplates = [\"overlay\"]\n".parse().unwrap();

            let effective = resolve_effective_config(&config, &tedge_config, Some(&overlay), None)
                .await
                .unwrap();

            let sourced = effective.get("smartrest.templates").unwrap_value();
            assert_eq!(sourced.value, "[\"local\"]");
            assert!(matches!(sourced.source, ConfigSource::MapperToml));
        }

        #[tokio::test]
        async fn unknown_key_returns_unknown_key() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            assert!(matches!(
                effective.get("nonexistent.key"),
                ConfigGetResult::UnknownKey
            ));
        }

        #[tokio::test]
        async fn schema_key_not_set_returns_not_set() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None); // url is None

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            // url has no default in tedge config — confirmed not set
            assert!(matches!(effective.get("url"), ConfigGetResult::NotSet));
            // credentials_path is None in the config and has no fallback
            assert!(matches!(
                effective.get("credentials_path"),
                ConfigGetResult::NotSet
            ));
        }

        #[tokio::test]
        async fn schema_key_shadows_raw_table_value() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            write_cert(&mapper_dir).await;
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "[device]\ncert_path = \"cert.pem\"\nkey_path = \"key.pem\"\n",
            )
            .await
            .unwrap();
            let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
            let tedge_config = TEdgeConfig::load_toml_str("");

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            let sourced = effective.get("device.cert_path").unwrap_value();
            // get() returns the resolved absolute path (schema value), not the raw relative string
            assert!(
                sourced.value.starts_with('/'),
                "expected absolute path, got: {}",
                sourced.value
            );
            assert!(matches!(
                sourced.source,
                ConfigSource::MapperTomlResolved { .. }
            ));
        }

        #[tokio::test]
        async fn schema_key_url_falls_through_to_overlay() {
            // Built-in mapper scenario: url is set in tedge.toml (overlay),
            // not in mapper.toml. get("url") should find it via the overlay.
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None); // url is None in mapper.toml
            let overlay: toml::Table = "url = \"mqtt.example.com:8883\"\n".parse().unwrap();

            let effective = resolve_effective_config(&config, &tedge_config, Some(&overlay), None)
                .await
                .unwrap();

            let sourced = effective.get("url").unwrap_value();
            assert_eq!(sourced.value, "mqtt.example.com:8883");
            assert!(matches!(sourced.source, ConfigSource::TedgeConfig));
        }

        #[tokio::test]
        async fn schema_key_credentials_path_falls_through_to_overlay() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let overlay: toml::Table = "credentials_path = \"/etc/tedge/credentials.toml\"\n"
                .parse()
                .unwrap();

            let effective = resolve_effective_config(&config, &tedge_config, Some(&overlay), None)
                .await
                .unwrap();

            let sourced = effective.get("credentials_path").unwrap_value();
            assert_eq!(sourced.value, "/etc/tedge/credentials.toml");
            assert!(matches!(sourced.source, ConfigSource::TedgeConfig));
        }

        #[tokio::test]
        async fn schema_key_not_set_without_overlay_still_returns_not_set() {
            // Without an overlay, a None typed field with no raw_table entry is NotSet.
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            assert!(matches!(effective.get("url"), ConfigGetResult::NotSet));
            assert!(matches!(
                effective.get("credentials_path"),
                ConfigGetResult::NotSet
            ));
        }
    }

    mod source_messages {
        use super::*;

        #[tokio::test]
        async fn absolute_cert_path_in_mapper_toml() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            let (cert, key) = write_cert(&mapper_dir).await;
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                format!("[device]\ncert_path = \"{cert}\"\nkey_path = \"{key}\"\n"),
            )
            .await
            .unwrap();
            let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
            let tedge_config = TEdgeConfig::load_toml_str("");

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            assert_eq!(
                effective.cert_path.unwrap().source.to_string(),
                "from mapper.toml"
            );
        }

        #[tokio::test]
        async fn relative_cert_path_in_mapper_toml() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            write_cert(&mapper_dir).await;
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "[device]\ncert_path = \"cert.pem\"\nkey_path = \"key.pem\"\n",
            )
            .await
            .unwrap();
            let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
            let tedge_config = TEdgeConfig::load_toml_str("");

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            assert_eq!(
                effective.cert_path.unwrap().source.to_string(),
                "relative path 'cert.pem' in mapper.toml, resolved to absolute"
            );
        }

        #[tokio::test]
        async fn cert_path_inherited_from_tedge_toml() {
            let ttd = TempTedgeDir::new();
            let (cert, key) = write_cert(ttd.utf8_path()).await;
            let tedge_config = TEdgeConfig::load_toml_str(&format!(
                "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
            ));
            let config = make_config(Some("mqtt.example.com:1883"));

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            assert_eq!(
                effective.cert_path.unwrap().source.to_string(),
                "not set in mapper.toml, inherited from tedge.toml"
            );
        }

        #[tokio::test]
        async fn device_id_inferred_from_cert_cn() {
            let ttd = TempTedgeDir::new();
            let (cert, key) = write_cert(ttd.utf8_path()).await;
            let tedge_config = TEdgeConfig::load_toml_str(&format!(
                "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
            ));
            let config = make_config(Some("mqtt.example.com:1883"));

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            assert_eq!(
                effective.device_id.unwrap().source.to_string(),
                format!("inferred from certificate CN ({cert})")
            );
        }

        #[tokio::test]
        async fn root_cert_path_uses_default() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            assert_eq!(
                effective.root_cert_path.source.to_string(),
                "schema default"
            );
        }

        #[tokio::test]
        async fn device_id_falls_back_to_tedge_toml() {
            let ttd = TempTedgeDir::new();
            let creds = ttd.utf8_path().join("creds.toml");
            tokio::fs::write(
                &creds,
                "[credentials]\nusername = \"u\"\npassword = \"p\"\n",
            )
            .await
            .unwrap();
            let tedge_config = TEdgeConfig::load_toml_str("device.id = \"root-device\"");
            let mut config = make_config(Some("mqtt.example.com:1883"));
            config.credentials_path = Some(creds);

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            assert_eq!(
                effective.device_id.unwrap().source.to_string(),
                "not set in mapper.toml, inherited from tedge.toml"
            );
        }

        #[tokio::test]
        async fn short_tags_match_expected_labels() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            let (cert, key) = write_cert(ttd.utf8_path()).await;
            let tedge_config = TEdgeConfig::load_toml_str(&format!(
                "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
            ));
            let config = make_config(Some("mqtt.example.com:1883"));

            let effective = resolve_effective_config(&config, &tedge_config, None, None)
                .await
                .unwrap();

            // cert CN → "cert CN"
            assert_eq!(effective.device_id.unwrap().source.short_tag(), "cert CN");
            // tedge.toml fallback → "tedge.toml"
            assert_eq!(
                effective.cert_path.unwrap().source.short_tag(),
                "tedge.toml"
            );
            // default → "default"
            assert_eq!(effective.root_cert_path.source.short_tag(), "default");
        }
    }

    mod to_template_table_fn {
        use super::*;

        #[tokio::test]
        async fn overlay_keys_survive_into_template_table() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let overlay: toml::Table = "[smartrest]\ntemplates = [\"12345\"]\n".parse().unwrap();

            let effective = resolve_effective_config(&config, &tedge_config, Some(&overlay), None)
                .await
                .unwrap();

            let table = effective.to_template_table();
            assert!(
                table
                    .get("smartrest")
                    .and_then(|v| v.as_table())
                    .and_then(|t| t.get("templates"))
                    .is_some(),
                "overlay key should be visible in template table"
            );
        }

        #[tokio::test]
        async fn cert_cn_overrides_overlay_device_id_in_template_table() {
            // The overlay (e.g. from the c8y config) may contain a device.id, but the
            // cert CN — computed during resolve — must take precedence.
            let ttd = TempTedgeDir::new();
            let (cert, key) = write_cert(ttd.utf8_path()).await;
            let tedge_config = TEdgeConfig::load_toml_str(&format!(
                "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
            ));
            let config = make_config(Some("mqtt.example.com:1883"));
            let overlay: toml::Table = "[device]\nid = \"overlay-id\"\n".parse().unwrap();

            let effective = resolve_effective_config(&config, &tedge_config, Some(&overlay), None)
                .await
                .unwrap();

            let table = effective.to_template_table();
            let id = table["device"]["id"].as_str().unwrap();
            assert_eq!(
                id, "localhost",
                "cert CN should take precedence over overlay device.id"
            );
        }
    }

    // ========================================================================
    // MapperConfigLookup integration tests
    // ========================================================================

    mod mapper_config_lookup {
        use super::*;
        use tedge_mqtt_bridge::config_toml::MapperArrayResult;
        use tedge_mqtt_bridge::config_toml::MapperConfigLookup;
        use tedge_mqtt_bridge::config_toml::MapperKeyResult;

        // Helper: resolve a custom mapper config (no overlay = custom mapper behaviour).
        async fn resolve_custom(
            config: &CustomMapperConfig,
            tedge_config: &TEdgeConfig,
        ) -> EffectiveMapperConfig {
            resolve_effective_config(config, tedge_config, None, None)
                .await
                .unwrap()
        }

        // Helper: resolve with a built-in overlay (is_builtin = true).
        async fn resolve_builtin(
            config: &CustomMapperConfig,
            tedge_config: &TEdgeConfig,
            overlay: &toml::Table,
        ) -> EffectiveMapperConfig {
            resolve_effective_config(config, tedge_config, Some(overlay), None)
                .await
                .unwrap()
                .with_mapper_name("c8y")
        }

        #[tokio::test]
        async fn custom_mapper_url_set_returns_value() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(Some("custom.example.com:8883"));
            let effective = resolve_custom(&config, &tedge_config).await;

            let result = effective.lookup_scalar("url");
            assert!(
                matches!(result, MapperKeyResult::Value(ref s) if s.contains("custom.example.com")),
                "expected Value, got {result:?}"
            );
        }

        #[tokio::test]
        async fn custom_mapper_url_unset_returns_not_set_with_mapper_toml_hint() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let effective = resolve_custom(&config, &tedge_config).await;

            let result = effective.lookup_scalar("url");
            let MapperKeyResult::NotSet { help, .. } = result else {
                panic!("expected NotSet, got {result:?}");
            };
            let help = help.expect("help text should be present");
            assert!(
                help.contains("mapper.toml"),
                "custom mapper help should mention mapper.toml: {help}"
            );
        }

        #[tokio::test]
        async fn builtin_mapper_url_unset_returns_not_set_with_tedge_config_hint() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let overlay = toml::Table::new();
            let effective = resolve_builtin(&config, &tedge_config, &overlay).await;

            let result = effective.lookup_scalar("url");
            let MapperKeyResult::NotSet { help, .. } = result else {
                panic!("expected NotSet, got {result:?}");
            };
            let help = help.expect("help text should be present");
            assert!(
                help.contains("tedge config set"),
                "built-in mapper help should mention 'tedge config set': {help}"
            );
        }

        #[tokio::test]
        async fn builtin_mapper_url_unset_display_key_has_mapper_prefix() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let overlay = toml::Table::new();
            let effective = resolve_builtin(&config, &tedge_config, &overlay).await;

            let result = effective.lookup_scalar("url");
            let MapperKeyResult::NotSet { display_key, .. } = result else {
                panic!("expected NotSet, got {result:?}");
            };
            assert_eq!(
                display_key.as_deref(),
                Some("c8y.url"),
                "built-in mapper display_key should be 'c8y.url'"
            );
        }

        #[tokio::test]
        async fn builtin_mapper_url_unset_help_contains_full_tedge_config_set_command() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let overlay = toml::Table::new();
            let effective = resolve_builtin(&config, &tedge_config, &overlay).await;

            let result = effective.lookup_scalar("url");
            let MapperKeyResult::NotSet { help, .. } = result else {
                panic!("expected NotSet, got {result:?}");
            };
            let help = help.expect("help text should be present");
            assert!(
                help.contains("tedge config set c8y.url"),
                "help should contain full 'tedge config set c8y.url <value>' command: {help}"
            );
        }

        #[tokio::test]
        async fn builtin_mapper_with_profile_url_unset_help_uses_profile_flag() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let overlay = toml::Table::new();
            let effective = resolve_effective_config(&config, &tedge_config, Some(&overlay), None)
                .await
                .unwrap()
                .with_mapper_name("c8y.prod");

            let result = effective.lookup_scalar("url");
            let MapperKeyResult::NotSet { help, display_key } = result else {
                panic!("expected NotSet, got {result:?}");
            };
            let help = help.expect("help text should be present");
            assert!(
                help.contains("--profile prod"),
                "help should contain '--profile prod': {help}"
            );
            assert!(
                help.contains("tedge config set c8y.url"),
                "help should contain 'tedge config set c8y.url': {help}"
            );
            assert_eq!(
                display_key.as_deref(),
                Some("c8y.prod.url"),
                "display_key should be 'c8y.prod.url'"
            );
        }

        #[tokio::test]
        async fn custom_mapper_url_unset_display_key_is_none() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let effective = resolve_custom(&config, &tedge_config).await;

            let result = effective.lookup_scalar("url");
            let MapperKeyResult::NotSet { display_key, .. } = result else {
                panic!("expected NotSet, got {result:?}");
            };
            assert_eq!(
                display_key, None,
                "custom mapper should not set display_key (no namespace prefix)"
            );
        }

        #[tokio::test]
        async fn builtin_mapper_url_set_in_overlay_returns_value() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let overlay: toml::Table = "url = \"overlay.example.com:8883\"\n".parse().unwrap();
            let effective = resolve_builtin(&config, &tedge_config, &overlay).await;

            let result = effective.lookup_scalar("url");
            assert!(
                matches!(result, MapperKeyResult::Value(ref s) if s.contains("overlay.example.com")),
                "expected Value from overlay, got {result:?}"
            );
        }

        #[tokio::test]
        async fn builtin_mapper_url_set_in_mapper_toml_takes_precedence_over_overlay() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(Some("mapper-toml.example.com:8883"));
            let overlay: toml::Table = "url = \"overlay.example.com:8883\"\n".parse().unwrap();
            let effective = resolve_builtin(&config, &tedge_config, &overlay).await;

            let result = effective.lookup_scalar("url");
            // The url field in make_config sets config.url, and the overlay also sets url.
            // The effective url is from config.url (mapper.toml), not the overlay.
            assert!(
                matches!(result, MapperKeyResult::Value(ref s) if s.contains("mapper-toml.example.com")),
                "mapper.toml url should take precedence over overlay, got {result:?}"
            );
        }

        #[tokio::test]
        async fn unknown_key_returns_unknown_key() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let effective = resolve_custom(&config, &tedge_config).await;

            let result = effective.lookup_scalar("this.key.does.not.exist");
            assert!(
                matches!(result, MapperKeyResult::UnknownKey),
                "expected UnknownKey, got {result:?}"
            );
        }

        #[tokio::test]
        async fn non_schema_key_from_mapper_toml_returns_value() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let mut config = make_config(None);
            config.table = "bridge.topic_prefix = \"custom/prefix\"\n".parse().unwrap();
            let effective = resolve_custom(&config, &tedge_config).await;

            let result = effective.lookup_scalar("bridge.topic_prefix");
            assert!(
                matches!(result, MapperKeyResult::Value(ref s) if s == "custom/prefix"),
                "expected Value, got {result:?}"
            );
        }

        #[tokio::test]
        async fn array_key_present_returns_values() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let mut config = make_config(None);
            config.table = "topics = [\"a\", \"b\", \"c\"]\n".parse().unwrap();
            let effective = resolve_custom(&config, &tedge_config).await;

            let result = effective.lookup_array("topics");
            assert!(
                matches!(result, MapperArrayResult::Values(ref v) if v == &["a", "b", "c"]),
                "expected Values([\"a\", \"b\", \"c\"]), got {result:?}"
            );
        }

        #[tokio::test]
        async fn array_schema_key_absent_returns_not_set() {
            // `url` is in the schema but has no configured value.
            // Arrays are looked up via raw_table, so "url" as an array is not found,
            // but it IS in the schema → NotSet.
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let effective = resolve_custom(&config, &tedge_config).await;

            let result = effective.lookup_array("url");
            assert!(
                matches!(result, MapperArrayResult::NotSet { .. }),
                "expected NotSet, got {result:?}"
            );
        }

        #[tokio::test]
        async fn array_non_schema_key_absent_returns_unknown_key() {
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let effective = resolve_custom(&config, &tedge_config).await;

            let result = effective.lookup_array("nonexistent.topics");
            assert!(
                matches!(result, MapperArrayResult::UnknownKey),
                "expected UnknownKey, got {result:?}"
            );
        }

        #[tokio::test]
        async fn device_id_from_explicit_mapper_toml_is_returned_when_cert_unavailable() {
            // Cert auth, no cert configured — device_id typed field is None.
            // But mapper.toml explicitly sets device.id, so raw_table has it.
            // lookup_scalar("device.id") should return Value from raw_table fallback.
            let tedge_config = TEdgeConfig::load_toml_str("");
            let mut config = make_config(None);
            config.table = "device.id = \"explicit-from-mapper\"\n".parse().unwrap();
            config.device = Some(crate::custom::config::DeviceConfig {
                id: Some("explicit-from-mapper".into()),
                cert_path: None,
                key_path: None,
                root_cert_path: None,
            });
            let effective = resolve_custom(&config, &tedge_config).await;

            let result = effective.lookup_scalar("device.id");
            assert!(
                matches!(result, MapperKeyResult::Value(ref s) if s == "explicit-from-mapper"),
                "expected Value from raw_table fallback, got {result:?}"
            );
        }
    }
}
