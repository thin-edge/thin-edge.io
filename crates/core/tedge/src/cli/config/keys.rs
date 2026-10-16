use std::fmt;
use std::str::FromStr;

use tedge_config::tedge_toml::ParseKeyError;
use tedge_config::tedge_toml::ReadableKey;
use tedge_config::tedge_toml::WritableKey;

#[derive(Debug, Clone)]
pub enum FederatedReadableKey {
    Core(ReadableKey),
    #[cfg(feature = "mapper-config")]
    Mapper(String),
}

impl fmt::Display for FederatedReadableKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(key) => write!(f, "{key}"),
            #[cfg(feature = "mapper-config")]
            Self::Mapper(key) => f.write_str(key),
        }
    }
}

impl FromStr for FederatedReadableKey {
    type Err = ParseKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        #[cfg(feature = "mapper-config")]
        if s.starts_with("mappers.") {
            return Ok(Self::Mapper(s.to_owned()));
        }

        ReadableKey::from_str(s).map(Self::Core)
    }
}

impl FederatedReadableKey {
    pub fn completions() -> Vec<clap_complete::CompletionCandidate> {
        #[cfg_attr(not(feature = "mapper-config"), expect(unused_mut))]
        let mut candidates = ReadableKey::completions();
        #[cfg(feature = "mapper-config")]
        candidates.extend(mapper_key_completions(&tedge_config::get_config_dir()));
        candidates
    }
}

#[derive(Debug, Clone)]
pub enum FederatedWritableKey {
    Core(WritableKey),
    #[cfg(feature = "mapper-config")]
    Mapper(String),
}

impl fmt::Display for FederatedWritableKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(key) => write!(f, "{}", key.to_cow_str()),
            #[cfg(feature = "mapper-config")]
            Self::Mapper(key) => f.write_str(key),
        }
    }
}

impl FromStr for FederatedWritableKey {
    type Err = ParseKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        #[cfg(feature = "mapper-config")]
        if s.starts_with("mappers.") {
            return Ok(Self::Mapper(s.to_owned()));
        }

        WritableKey::from_str(s).map(Self::Core)
    }
}

impl FederatedWritableKey {
    pub fn completions() -> Vec<clap_complete::CompletionCandidate> {
        #[cfg_attr(not(feature = "mapper-config"), expect(unused_mut))]
        let mut candidates = WritableKey::completions();
        #[cfg(feature = "mapper-config")]
        candidates.extend(mapper_key_completions(&tedge_config::get_config_dir()));
        candidates
    }
}

#[cfg(feature = "mapper-config")]
fn mapper_key_completions(config_dir: &std::path::Path) -> Vec<clap_complete::CompletionCandidate> {
    let Ok(config_dir) = camino::Utf8PathBuf::try_from(config_dir.to_owned()) else {
        return Vec::new();
    };
    let Ok(fed) = tedge_mapper_config::load_federated_config(&config_dir) else {
        return Vec::new();
    };
    let mounted_prefixes = fed.mount_prefixes();
    let mut entries = fed.all_entries();

    for name in tedge_mapper_config::BUILTIN_MAPPER_NAMES {
        let prefix = format!("mappers.{name}.");
        if !mounted_prefixes.contains(&prefix) {
            entries.extend(tedge_mapper_config::builtin_mapper_entries(
                &config_dir,
                name,
            ));
        }
    }

    entries
        .into_iter()
        .filter(|e| e.key.starts_with("mappers."))
        .map(|e| {
            let mut c = clap_complete::CompletionCandidate::new(e.key);
            if !e.doc.is_empty() {
                c = c.help(Some(e.doc.join(" ").into()));
            }
            c
        })
        .collect()
}

#[cfg(all(test, feature = "mapper-config"))]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    #[test]
    fn mapper_keys_are_completed_for_mapper_directories_and_builtin_clouds() {
        let dir = TempTedgeDir::new();
        dir.file("tedge.toml");
        dir.dir("mappers").dir("thingsboard").file("mapper.toml");

        let completions: Vec<String> = mapper_key_completions(dir.std_path())
            .iter()
            .map(|c| c.get_value().to_string_lossy().into_owned())
            .collect();

        for key in [
            "mappers.thingsboard.url",
            "mappers.c8y.url",
            "mappers.aws.url",
        ] {
            assert!(
                completions.iter().any(|c| c == key),
                "{key} should be completed: {completions:?}"
            );
        }
        assert!(
            completions.iter().all(|c| c.starts_with("mappers.")),
            "only mapper keys should be completed: {completions:?}"
        );
    }

    #[test]
    fn readable_and_writable_key_completions_include_core_keys() {
        for completions in [
            FederatedReadableKey::completions(),
            FederatedWritableKey::completions(),
        ] {
            assert!(
                completions.iter().any(|c| c.get_value() == "device.type"),
                "device.type should be completed"
            );
        }
    }
}
