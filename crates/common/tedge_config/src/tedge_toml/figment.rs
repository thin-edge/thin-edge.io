use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt::Display;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

use figment::providers::Format;
use figment::providers::Toml;
use figment::util::nest;
use figment::value::Dict;
use figment::value::Map;
use figment::value::Uncased;
use figment::value::Value;
use figment::Figment;
use figment::Metadata;
use figment::Profile;
use figment::Provider;
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;

use crate::TEdgeConfigError;

pub trait ConfigSources {
    const INCLUDE_ENVIRONMENT: bool;
}

#[derive(Clone, Debug)]
pub struct FileAndEnvironment;
#[derive(Clone, Debug)]
pub struct FileOnly;

impl ConfigSources for FileAndEnvironment {
    const INCLUDE_ENVIRONMENT: bool = true;
}

impl ConfigSources for FileOnly {
    const INCLUDE_ENVIRONMENT: bool = false;
}

#[derive(Default, Debug, PartialEq, Eq)]
#[must_use]
pub struct UnusedValueWarnings(Vec<String>);

impl UnusedValueWarnings {
    pub fn emit(self) {
        for warning in self.0 {
            tracing::warn!("{warning}");
        }
    }
}

/// Extract the configuration data from the provided TOML path and `TEDGE_` prefixed environment variables
pub fn extract_data<T: DeserializeOwned, Sources: ConfigSources>(
    path: impl AsRef<Path>,
) -> Result<(T, UnusedValueWarnings), TEdgeConfigError> {
    let env = TEdgeEnv::default();
    let figment = Figment::new().merge(Toml::file(path));

    let figment = if Sources::INCLUDE_ENVIRONMENT {
        figment.merge(env.provider())
    } else {
        figment
    };

    let data = extract_exact(&figment, &env);

    let warnings = unused_value_warnings::<T>(&figment, &env)
        .ok()
        .map(UnusedValueWarnings)
        .unwrap_or_default();

    match data {
        Ok(data) => Ok((data, warnings)),
        Err(e) => {
            warnings.emit();
            Err(e)
        }
    }
}

#[cfg(feature = "test")]
pub fn extract_from_toml_str<T: DeserializeOwned>(toml: &str) -> Result<T, TEdgeConfigError> {
    let env = TEdgeEnv::default();
    let figment = Figment::new().merge(Toml::string(toml));

    let data = extract_exact(&figment, &env);

    let warnings = unused_value_warnings::<T>(&figment, &env)
        .ok()
        .map(UnusedValueWarnings)
        .unwrap_or_default();

    warnings.emit();
    data
}

fn unused_value_warnings<T: DeserializeOwned>(
    figment: &Figment,
    env: &TEdgeEnv,
) -> Result<Vec<String>, TEdgeConfigError> {
    let mut warnings = Vec::new();

    let de = extract_exact::<figment::value::Value>(figment, env)?;

    let _: T = serde_ignored::deserialize(&de, |path| {
        let serde_path = path.to_string();

        let source = figment
            .find_metadata(&serde_path)
            .and_then(|metadata| ConfigurationSource::infer(env, &serde_path, metadata));

        if let Some(source) = source {
            warnings.push(format!(
                "Unknown configuration field {serde_path:?} from {source}"
            ));
        } else {
            warnings.push(format!("Unknown configuration field {serde_path:?}"));
        }
    })
    .map_err(TEdgeConfigError::Figment)?;

    Ok(warnings)
}

fn extract_exact<T: DeserializeOwned>(
    figment: &Figment,
    env: &TEdgeEnv,
) -> Result<T, TEdgeConfigError> {
    figment.extract().map_err(|error_list| {
        TEdgeConfigError::multiple_errors(
            error_list
                .into_iter()
                .map(|error| add_error_context(error, env))
                .collect(),
        )
    })
}

fn add_error_context(mut error: figment::Error, env: &TEdgeEnv) -> TEdgeConfigError {
    use ConfigurationSource::*;
    if let Some(ref mut metadata) = error.metadata {
        match ConfigurationSource::infer(env, &error.path.join("."), metadata) {
            Some(EnvVariable(variable)) => {
                metadata.name = Cow::Owned(format!("{variable} environment variable"));
            }
            Some(TomlFile(_)) => {
                // Ignore the profile field, we don't use it for anything
                *metadata = metadata
                    .clone()
                    .interpolater(|_profile, path| path.join("."));
            }
            _ => (),
        };
    }

    TEdgeConfigError::Figment(error)
}

enum ConfigurationSource {
    TomlFile(PathBuf),
    EnvVariable(String),
    Unknown(String),
}

impl ConfigurationSource {
    fn infer(env: &TEdgeEnv, path: &str, m: &Metadata) -> Option<Self> {
        let ret = m
            .source
            .as_ref()
            // If we have a path, it must have come from a file
            .and_then(|source| source.file_path().map(<_>::to_owned).map(Self::TomlFile))
            // Failing that, try and find a corresponding environment variable
            .or_else(|| env.variable_name(path).map(Self::EnvVariable))
            .or_else(|| Some(Self::Unknown(m.name.clone().into_owned())));

        ret
    }
}

impl Display for ConfigurationSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TomlFile(path) => write!(f, "TOML file {}", path.display()),
            Self::EnvVariable(variable) => write!(f, "environment variable {variable}"),
            Self::Unknown(name) => write!(f, "{name}"),
        }
    }
}

struct TEdgeEnv {
    prefix: &'static str,
    separator: &'static str,
}

impl Default for TEdgeEnv {
    fn default() -> Self {
        Self {
            prefix: "TEDGE_",
            separator: "_",
        }
    }
}

impl TEdgeEnv {
    fn variable_name(&self, key: &str) -> Option<String> {
        let desired_key = key.replace('.', self.separator);
        std::env::vars_os().find_map(|(k, _)| {
            k.to_str()?
                .strip_prefix(self.prefix)
                .filter(|key| key.eq_ignore_ascii_case(&desired_key))
                .map(|name| format!("{}{}", self.prefix, name))
        })
    }

    fn provider(&self) -> TEdgeEnvProvider {
        let prefix = self.prefix;
        static WARNINGS: Lazy<Mutex<HashSet<String>>> = Lazy::new(<_>::default);
        let provider = figment::providers::Env::prefixed(self.prefix).ignore(&["CONFIG_DIR", "CLOUD_PROFILE"])
        .filter(move |key| {
            std::env::vars()
            .find(|(k, _)| k.strip_prefix(prefix).is_some_and(|k| k == key))
            .map_or(true, |(_, val)| !val.is_empty())})
        .map(move |name| {
            let lowercase_name = name.as_str().to_ascii_lowercase();
            Uncased::new(
                tracing::subscriber::with_default(
                    tracing::subscriber::NoSubscriber::default(),
                    || lowercase_name.parse::<crate::tedge_toml::WritableKey>(),
                )
                .map(|key| key.to_string())
                .map_err(|err| {
                    let is_read_only_key = matches!(err, crate::tedge_toml::ParseKeyError::ReadOnly(_));
                    if is_read_only_key && !WARNINGS.lock().unwrap().insert(lowercase_name.clone()) {
                        tracing::error!(
                            "Failed to configure tedge with environment variable `TEDGE_{name}`: {}",
                            err.to_string().replace('\n', " ")
                        )
                    }
                })
                .unwrap_or(lowercase_name),
            )
        });
        TEdgeEnvProvider { inner: provider }
    }
}

struct TEdgeEnvProvider {
    inner: figment::providers::Env,
}

impl Provider for TEdgeEnvProvider {
    fn metadata(&self) -> Metadata {
        self.inner.metadata()
    }

    fn data(&self) -> Result<figment::value::Map<figment::Profile, Dict>, figment::Error> {
        let mut dict = Dict::new();
        for (k, v) in self.inner.iter() {
            let value = if v.len() > 1 && v.starts_with('0') && !v.contains('.') {
                Value::from(v)
            } else {
                v.parse().expect("infallible")
            };
            let nested_dict = nest(k.as_str(), value)
                .into_dict()
                .expect("key is non-empty: must have dict");

            dict = dict.merge(nested_dict);
        }

        Ok(self.inner.profile.collect(dict))
    }
}

pub trait Mergeable: Sized {
    fn merge(self, other: Self) -> Self;
}

impl Mergeable for Profile {
    fn merge(self, other: Self) -> Self {
        other
    }
}

impl Mergeable for Value {
    fn merge(self, other: Self) -> Self {
        use Value::Dict as D;
        match (self, other) {
            (D(_, a), D(t, b)) => D(t, a.merge(b)),
            (_, v) => v,
        }
    }
}

impl<K: Eq + std::hash::Hash + Ord, V: Mergeable> Mergeable for Map<K, V> {
    fn merge(self, mut other: Self) -> Self {
        let mut joined = Map::new();
        for (a_key, a_val) in self {
            match other.remove(&a_key) {
                Some(b_val) => joined.insert(a_key, a_val.merge(b_val)),
                None => joined.insert(a_key, a_val),
            };
        }

        // `b` contains `b - a`, i.e, additions. keep them all.
        joined.extend(other);
        joined
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::tedge_toml::AppendRemoveItem;
    use crate::tedge_toml::ReadError;
    use serde::Deserialize;
    use tedge_config_macros::define_tedge_config;

    use super::*;

    #[test]
    fn config_dir_environment_variable_does_not_generate_a_warning() {
        #[derive(Deserialize)]
        struct Config {}

        figment::Jail::expect_with(|jail| {
            jail.set_env("TEDGE_CONFIG_DIR", "/etc/moved-tedge");

            let env = TEdgeEnv::default();
            let figment = Figment::new()
                .merge(Toml::file("tedge.toml"))
                .merge(env.provider());

            let warnings = unused_value_warnings::<Config>(&figment, &env).unwrap();
            assert_eq!(dbg!(warnings).len(), 0);

            Ok(())
        })
    }

    #[test]
    fn integer_environment_variables_are_parsed_as_integers() {
        #[derive(Deserialize)]
        struct Config {
            value: u32,
        }

        figment::Jail::expect_with(|jail| {
            jail.set_env("TEDGE_VALUE", "1234");

            assert_eq!(
                extract_data::<Config, FileAndEnvironment>(&PathBuf::from("tedge.toml"))
                    .unwrap()
                    .0
                    .value,
                1234
            );
            Ok(())
        })
    }

    #[test]
    fn environment_variables_with_leading_zeroes_are_parsed_as_strings() {
        #[derive(Deserialize)]
        struct Config {
            value: String,
        }

        figment::Jail::expect_with(|jail| {
            jail.set_env("TEDGE_VALUE", "01234");

            assert_eq!(
                extract_data::<Config, FileAndEnvironment>(&PathBuf::from("tedge.toml"))
                    .unwrap()
                    .0
                    .value,
                "01234",
            );
            Ok(())
        })
    }

    #[test]
    fn environment_variable_zero_is_parsed_as_number() {
        #[derive(Deserialize)]
        struct Config {
            value: u32,
        }

        figment::Jail::expect_with(|jail| {
            jail.set_env("TEDGE_VALUE", "0");

            assert_eq!(
                extract_data::<Config, FileAndEnvironment>(&PathBuf::from("tedge.toml"))
                    .unwrap()
                    .0
                    .value,
                0,
            );
            Ok(())
        })
    }

    #[test]
    fn environment_variable_float_is_parsed_as_number() {
        #[derive(Deserialize)]
        struct Config {
            value: f64,
        }

        figment::Jail::expect_with(|jail| {
            jail.set_env("TEDGE_VALUE", "0.123");

            assert_eq!(
                extract_data::<Config, FileAndEnvironment>(&PathBuf::from("tedge.toml"))
                    .unwrap()
                    .0
                    .value,
                0.123,
            );
            Ok(())
        })
    }

    #[test]
    fn environment_variables_override_config_file() {
        #[derive(Deserialize)]
        struct Config {
            c8y: C8yConfig,
        }

        #[derive(Deserialize)]
        struct C8yConfig {
            url: String,
        }

        figment::Jail::expect_with(|jail| {
            jail.create_file(
                "tedge.toml",
                r#"
            [c8y]
            url = "test.c8y.io"
            "#,
            )?;

            jail.set_env("TEDGE_C8Y_URL", "override.c8y.io");

            assert_eq!(
                extract_data::<Config, FileAndEnvironment>(&PathBuf::from("tedge.toml"))
                    .unwrap()
                    .0
                    .c8y
                    .url,
                "override.c8y.io"
            );
            Ok(())
        })
    }

    #[test]
    fn specifies_file_name_and_variable_path_in_relevant_warnings() {
        #[derive(Deserialize)]
        #[allow(unused)]
        struct Config {
            some: Inner,
        }
        #[derive(Deserialize)]
        struct Inner {}

        figment::Jail::expect_with(|jail| {
            jail.create_file("tedge.toml", r#"some = { value = "test.c8y.io" }"#)?;
            let env = TEdgeEnv::default();
            let figment = Figment::new()
                .merge(Toml::file("tedge.toml"))
                .merge(env.provider());

            let warnings = unused_value_warnings::<Config>(&figment, &env).unwrap();
            assert_eq!(warnings.len(), 1);
            let warning = dbg!(warnings.first().unwrap());
            assert!(warning.contains("some.value"));
            assert!(warning.contains("tedge.toml"));
            Ok(())
        })
    }

    #[test]
    fn specifies_environment_variable_name_in_relevant_warnings() {
        #[derive(Deserialize)]
        struct EmptyConfig {}

        figment::Jail::expect_with(|jail| {
            let variable_name = "TEDGE_MightAsWellCheckCasingToo";
            jail.set_env(variable_name, "Some value");
            let env = TEdgeEnv::default();

            let figment = Figment::new().merge(env.provider());

            let warnings = unused_value_warnings::<EmptyConfig>(&figment, &env).unwrap();
            assert_eq!(warnings.len(), 1);
            let warning = dbg!(warnings.first().unwrap());
            assert!(warning.contains(variable_name));
            Ok(())
        })
    }

    #[test]
    fn specifies_environment_variable_name_in_relevant_errors() {
        #[derive(Deserialize, Debug)]
        #[allow(unused)]
        struct Config {
            value: String,
        }

        figment::Jail::expect_with(|jail| {
            let variable_name = "TEDGE_VALUE";
            jail.set_env(variable_name, "123");

            let errors = extract_data::<Config, FileAndEnvironment>("tedge.toml").unwrap_err();
            assert!(dbg!(errors.to_string()).contains(variable_name));
            Ok(())
        })
    }

    #[test]
    fn ignores_environment_variable_if_in_file_only_mode() {
        #[derive(Deserialize, Debug)]
        #[allow(unused)]
        struct Config {
            value: String,
        }

        figment::Jail::expect_with(|jail| {
            jail.create_file("tedge.toml", "value = \"config\"")?;
            let variable_name = "TEDGE_VALUE";
            jail.set_env(variable_name, "environment");

            let data = extract_data::<Config, FileOnly>("tedge.toml").unwrap();
            assert_eq!(data.0.value, "config");
            Ok(())
        })
    }

    #[test]
    fn environment_variables_can_override_profiled_configurations() {
        use tedge_config_macros::*;

        define_tedge_config!(
            #[tedge_config(multi)]
            c8y: {
                url: String,
            }
        );

        figment::Jail::expect_with(|jail| {
            jail.create_file(
                "tedge.toml",
                r#"
            [c8y.profiles.test]
            url = "test.c8y.io"
            "#,
            )?;

            jail.set_env("TEDGE_C8Y_PROFILES_TEST_URL", "override.c8y.io");

            let dto =
                extract_data::<TEdgeConfigDto, FileAndEnvironment>(&PathBuf::from("tedge.toml"))
                    .unwrap()
                    .0;
            assert_eq!(
                dto.c8y.try_get(Some("test"), "c8y").unwrap().url.as_deref(),
                Some("override.c8y.io")
            );
            Ok(())
        })
    }

    #[test]
    fn empty_environment_variables_are_ignored() {
        use tedge_config_macros::*;

        define_tedge_config!(
            c8y: {
                url: String,
            }
        );

        figment::Jail::expect_with(|jail| {
            jail.create_file(
                "tedge.toml",
                r#"
            [c8y]
            url = "test.c8y.io"
            "#,
            )?;

            jail.set_env("TEDGE_C8Y_URL", "");

            let dto =
                extract_data::<TEdgeConfigDto, FileAndEnvironment>(&PathBuf::from("tedge.toml"))
                    .unwrap()
                    .0;
            assert_eq!(dto.c8y.url.as_deref(), Some("test.c8y.io"));
            Ok(())
        })
    }
    #[test]
    fn environment_variable_profile_warnings_use_key_with_correct_format() {
        use tedge_config_macros::*;

        define_tedge_config!(
            #[tedge_config(multi)]
            c8y: {
                url: String,
            }
        );

        figment::Jail::expect_with(|jail| {
            jail.set_env("TEDGE_C8Y_PROFILES_TEST_UNKNOWN", "override.c8y.io");

            let warnings =
                extract_data::<TEdgeConfigDto, FileAndEnvironment>(&PathBuf::from("tedge.toml"))
                    .unwrap()
                    .1;
            assert_eq!(
                warnings.0,
                ["Unknown configuration field \"c8y_profiles_test_unknown\" from environment variable TEDGE_C8Y_PROFILES_TEST_UNKNOWN"]
            );
            Ok(())
        })
    }

    #[test]
    fn toml_profile_warnings_use_key_with_correct_format() {
        use tedge_config_macros::*;

        define_tedge_config!(
            #[tedge_config(multi)]
            c8y: {
                url: String,
            }
        );

        figment::Jail::expect_with(|jail| {
            jail.create_file(
                "tedge.toml",
                r#"
            [c8y.profiles.test]
            unknown = "test.c8y.io"
            "#,
            )?;
            let warnings =
                extract_data::<TEdgeConfigDto, FileAndEnvironment>(&PathBuf::from("tedge.toml"))
                    .unwrap()
                    .1;
            assert!(dbg!(warnings.0.first().unwrap()).contains("c8y.profiles.test.unknown"));
            Ok(())
        })
    }
}
