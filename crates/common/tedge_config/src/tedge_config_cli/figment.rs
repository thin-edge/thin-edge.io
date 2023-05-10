use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt::Display;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

use figment::providers::Format;
use figment::providers::Toml;
use figment::value::Uncased;
use figment::Figment;
use figment::Metadata;
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

    fn provider(&self) -> figment::providers::Env {
        static WARNINGS: Lazy<Mutex<HashSet<String>>> = Lazy::new(<_>::default);
        figment::providers::Env::prefixed(self.prefix).map(move |name| {
            let lowercase_name = name.as_str().to_ascii_lowercase();
            Uncased::new(
                tracing::subscriber::with_default(
                    tracing::subscriber::NoSubscriber::default(),
                    || lowercase_name.parse::<crate::new::WritableKey>(),
                )
                .map(|key| key.as_str().to_owned())
                .map_err(|err| {
                    let is_read_only_key = matches!(err, crate::new::ParseKeyError::ReadOnly(_));
                    if is_read_only_key && !WARNINGS.lock().unwrap().insert(lowercase_name.clone()) {
                            tracing::error!(
                                "Failed to configure tedge with environment variable `TEDGE_{name}`: {}",
                                err.to_string().replace('\n', " ")
                            )
         }
                })
                .unwrap_or(lowercase_name),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde::Deserialize;

    use super::*;

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
}
