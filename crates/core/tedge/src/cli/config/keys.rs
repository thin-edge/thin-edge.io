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
        candidates.extend(mapper_key_completions());
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
        candidates.extend(mapper_key_completions());
        candidates
    }
}

#[cfg(feature = "mapper-config")]
fn mapper_key_completions() -> Vec<clap_complete::CompletionCandidate> {
    let config_dir = tedge_config::get_config_dir();
    let Ok(fed) = tedge_mapper_config::load_federated_config(&config_dir) else {
        return Vec::new();
    };
    fed.all_entries()
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
