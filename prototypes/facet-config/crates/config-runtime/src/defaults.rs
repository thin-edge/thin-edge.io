use crate::reflect::{config_get, config_set, config_unset, ConfigError};
use facet::Facet;

/// Describes how to produce a value when a config key is not set directly.
pub enum DefaultSpec {
    /// A fixed default value stored as the same string accepted by `config set`.
    Value(String),

    /// A fresh default computed each time the key is read.
    Function(fn() -> String),

    /// A required fallback to another key in the same DTO.
    FromKey(&'static str),

    /// An optional fallback to another key in the same DTO.
    FromOptionalKey(&'static str),

    /// A fallback to a key owned by the root config.
    FromRoot(&'static str),
}

/// Associates a config key with its defaulting rule.
pub struct FieldDefault {
    pub key: &'static str,
    pub spec: DefaultSpec,
}

/// Defaulting rules for one config schema.
pub struct DefaultsRegistry {
    defaults: Vec<FieldDefault>,
}

/// Callback used by mounted configs to read fallback values from the root config.
pub type RootResolver<'a> = Option<&'a dyn Fn(&str) -> Option<String>>;

/// Environment variables captured as data before being applied to a config DTO.
pub struct EnvOverrides {
    vars: Vec<(String, String)>,
}

impl DefaultsRegistry {
    /// Creates a registry and rejects impossible required fallback chains.
    pub fn new(defaults: Vec<FieldDefault>) -> Result<Self, String> {
        let registry = Self { defaults };
        registry.validate()?;
        Ok(registry)
    }

    pub fn get(&self, key: &str) -> Option<&DefaultSpec> {
        self.defaults.iter().find(|d| d.key == key).map(|d| &d.spec)
    }

    pub fn root_defaults(&self) -> Vec<(&str, &str)> {
        self.defaults
            .iter()
            .filter_map(|d| match &d.spec {
                DefaultSpec::FromRoot(root_key) => Some((d.key, *root_key)),
                _ => None,
            })
            .collect()
    }

    fn validate(&self) -> Result<(), String> {
        for field in &self.defaults {
            if let DefaultSpec::FromKey(source_key) = &field.spec {
                if !self.is_resolvable(source_key, 0) {
                    return Err(format!(
                        "FromKey default for '{}' references '{}', which has no default and may not be set",
                        field.key, source_key
                    ));
                }
            }
        }
        Ok(())
    }

    fn is_resolvable(&self, key: &str, depth: usize) -> bool {
        if depth > 10 {
            return false;
        }
        match self.get(key) {
            Some(DefaultSpec::Value(_) | DefaultSpec::Function(_)) => true,
            Some(DefaultSpec::FromKey(source)) => self.is_resolvable(source, depth + 1),
            Some(DefaultSpec::FromOptionalKey(_) | DefaultSpec::FromRoot(_)) => true,
            None => false,
        }
    }
}

impl EnvOverrides {
    /// Captures the current process environment.
    pub fn from_env() -> Self {
        Self {
            vars: std::env::vars().collect(),
        }
    }

    /// Builds overrides from explicit pairs, usually for tests or controlled callers.
    pub fn from_pairs(vars: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            vars: vars.into_iter().collect(),
        }
    }

    /// Applies variables under `prefix`, translating env-style names to known keys.
    pub fn apply_with_prefix<T: for<'a> Facet<'a>>(
        &self,
        dto: &mut T,
        prefix: &str,
        known_keys: &[String],
    ) -> Vec<String> {
        self.apply_with_prefix_excluding(dto, prefix, known_keys, &[])
    }

    /// Applies variables under `prefix`, except names claimed by more specific prefixes.
    pub fn apply_with_prefix_excluding<T: for<'a> Facet<'a>>(
        &self,
        dto: &mut T,
        prefix: &str,
        known_keys: &[String],
        exclude_prefixes: &[&str],
    ) -> Vec<String> {
        let mut warnings = Vec::new();

        for (env_name, value) in &self.vars {
            let Some(raw_key) = env_name.strip_prefix(prefix) else {
                continue;
            };

            if exclude_prefixes.iter().any(|ep| env_name.starts_with(ep)) {
                continue;
            }

            let raw_key = raw_key.to_ascii_lowercase();

            let Some(resolved) = resolve_env_key(&raw_key, known_keys) else {
                warnings.push(format!(
                    "Unknown configuration field \"{raw_key}\" from environment variable {env_name}"
                ));
                continue;
            };

            apply_env_value(dto, &resolved, value, env_name, &mut warnings);
        }

        warnings
    }

    /// Applies environment variables for a cloud mapper such as `c8y` or `aws`.
    ///
    /// Unprofiled: `TEDGE_C8Y_URL` → key `url` (applied when `profile` is None).
    /// Profiled: `TEDGE_C8Y_PROFILES_STAGING_URL` → key `url` (applied when `profile` is Some("staging")).
    pub fn apply_for_cloud<T: for<'a> Facet<'a>>(
        &self,
        dto: &mut T,
        cloud: &str,
        profile: Option<&str>,
        known_keys: &[String],
    ) -> Vec<String> {
        let base_prefix = format!("TEDGE_{}_", cloud.to_ascii_uppercase());
        let profile_prefix = format!("TEDGE_{}_PROFILES_", cloud.to_ascii_uppercase());
        let mut warnings = Vec::new();

        for (env_name, value) in &self.vars {
            if let Some(raw_suffix) = env_name.strip_prefix(&profile_prefix) {
                let raw_suffix = raw_suffix.to_ascii_lowercase();

                if let Some(underscore_pos) = raw_suffix.find('_') {
                    let candidate_profile = &raw_suffix[..underscore_pos];
                    let rest = &raw_suffix[underscore_pos + 1..];

                    if let Some(resolved_key) = resolve_env_key(rest, known_keys) {
                        if profile == Some(candidate_profile) {
                            apply_env_value(dto, &resolved_key, value, env_name, &mut warnings);
                        }
                        continue;
                    }
                }

                warnings.push(format!(
                    "Unknown configuration field from environment variable {env_name}"
                ));
            } else if let Some(raw_suffix) = env_name.strip_prefix(&base_prefix) {
                let raw_suffix = raw_suffix.to_ascii_lowercase();

                if let Some(resolved_key) = resolve_env_key(&raw_suffix, known_keys) {
                    if profile.is_none() {
                        apply_env_value(dto, &resolved_key, value, env_name, &mut warnings);
                    }
                    continue;
                }

                warnings.push(format!(
                    "Unknown configuration field from environment variable {env_name}"
                ));
            }
        }

        warnings
    }
}

/// Reads a value from a DTO, applying the key's default rule when unset.
pub fn config_get_with_defaults<T: for<'a> Facet<'a>>(
    dto: &T,
    key: &str,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
) -> Result<Option<String>, ConfigError> {
    config_get_with_defaults_inner(dto, key, defaults, root_resolver, 0)
}

/// Resolves an env suffix by treating underscores as either literal `_` or `.`.
pub fn resolve_env_key(raw: &str, known_keys: &[String]) -> Option<String> {
    let underscores: Vec<usize> = raw
        .char_indices()
        .filter(|(_, c)| *c == '_')
        .map(|(i, _)| i)
        .collect();

    if underscores.is_empty() {
        return known_keys.iter().find(|k| k.as_str() == raw).cloned();
    }

    let mut candidate = raw.as_bytes().to_vec();
    resolve_recursive(&mut candidate, &underscores, 0, known_keys)
}

fn config_get_with_defaults_inner<T: for<'a> Facet<'a>>(
    dto: &T,
    key: &str,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
    depth: usize,
) -> Result<Option<String>, ConfigError> {
    if depth > 10 {
        return Err(ConfigError::ReflectError(format!(
            "Cycle detected resolving defaults for '{key}'"
        )));
    }

    let value = config_get(dto, key)?;
    if value.is_some() {
        return Ok(value);
    }

    let Some(spec) = defaults.get(key) else {
        return Ok(None);
    };

    match spec {
        DefaultSpec::Value(v) => Ok(Some(v.clone())),
        DefaultSpec::Function(f) => Ok(Some(f())),
        DefaultSpec::FromKey(source_key) => {
            let resolved = config_get_with_defaults_inner(
                dto,
                source_key,
                defaults,
                root_resolver,
                depth + 1,
            )?;
            match resolved {
                Some(v) => Ok(Some(v)),
                None => Err(ConfigError::ReflectError(format!(
                    "'{key}' defaults to '{source_key}', but '{source_key}' is also not set"
                ))),
            }
        }
        DefaultSpec::FromOptionalKey(source_key) => {
            config_get_with_defaults_inner(dto, source_key, defaults, root_resolver, depth + 1)
        }
        DefaultSpec::FromRoot(root_key) => Ok(root_resolver.and_then(|resolve| resolve(root_key))),
    }
}

fn apply_env_value<T: for<'a> Facet<'a>>(
    dto: &mut T,
    key: &str,
    value: &str,
    env_name: &str,
    warnings: &mut Vec<String>,
) {
    if value.is_empty() {
        let _ = config_unset(dto, key);
    } else if let Err(e) = config_set(dto, key, value) {
        warnings.push(format!("Failed to apply {env_name}={value}: {e}"));
    }
}

fn resolve_recursive(
    candidate: &mut Vec<u8>,
    underscores: &[usize],
    idx: usize,
    known_keys: &[String],
) -> Option<String> {
    if idx == underscores.len() {
        let s = std::str::from_utf8(candidate).ok()?;
        return known_keys.iter().find(|k| k.as_str() == s).cloned();
    }

    let pos = underscores[idx];

    for replacement in [b'_', b'.'] {
        candidate[pos] = replacement;
        let prefix = std::str::from_utf8(&candidate[..=pos]).ok()?;
        if known_keys.iter().any(|k| k.starts_with(prefix)) {
            if let Some(result) = resolve_recursive(candidate, underscores, idx + 1, known_keys) {
                return Some(result);
            }
        }
    }

    candidate[pos] = b'_';
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_env_key_maps_underscore_to_dot_separator() {
        let keys = vec!["mqtt.port".into(), "mqtt.host".into()];
        assert_eq!(
            resolve_env_key("mqtt_port", &keys),
            Some("mqtt.port".into())
        );
    }

    #[test]
    fn resolve_ambiguous_env_key_prefers_valid_path() {
        let keys = vec!["mqtt.bind_address".into(), "mqtt.host".into()];
        assert_eq!(
            resolve_env_key("mqtt_bind_address", &keys),
            Some("mqtt.bind_address".into())
        );
    }

    #[test]
    fn resolve_env_key_handles_multiple_dot_levels() {
        let keys = vec![
            "c8y.proxy.bind.port".into(),
            "c8y.proxy.bind.address".into(),
        ];
        assert_eq!(
            resolve_env_key("c8y_proxy_bind_port", &keys),
            Some("c8y.proxy.bind.port".into())
        );
    }

    #[test]
    fn resolve_unknown_env_key_returns_none() {
        let keys = vec!["mqtt.port".into()];
        assert_eq!(resolve_env_key("unknown_key", &keys), None);
    }

    #[test]
    fn resolve_key_with_25_underscores() {
        let raw = "a_b_c_d_e_f_g_h_i_j_k_l_m_n_o_p_q_r_s_t_u_v_w_x_y_z";
        let dotted: String = raw.replace('_', ".");
        let keys = vec![dotted.clone()];
        assert_eq!(resolve_env_key_or_timeout(raw, &keys), Some(dotted));
    }

    #[test]
    fn no_match_with_25_underscores() {
        let raw = "a_b_c_d_e_f_g_h_i_j_k_l_m_n_o_p_q_r_s_t_u_v_w_x_y_z";
        let keys = vec!["not.a.match".into()];
        assert_eq!(resolve_env_key_or_timeout(raw, &keys), None);
    }

    fn resolve_env_key_or_timeout(raw: &str, keys: &[String]) -> Option<String> {
        let raw = raw.to_owned();
        let keys = keys.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(resolve_env_key(&raw, &keys));
        });
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .expect("resolve_env_key timed out — likely exponential blowup")
    }
}
