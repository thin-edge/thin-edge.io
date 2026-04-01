use crate::LoadError;
use camino::Utf8Path;
use serde_json::Map;
use serde_json::Value;
use std::collections::HashMap;
use tokio::fs::read_to_string;
use tracing::log;

/// The mapper.toml file of the mapper running the flows
///
/// The content of this file is used a simple map of string values indexed by config paths,
/// any value derivation having been done beforehand (e.g. reading the `device.id` from the device certificate).
///
/// A flow definition can reference a mapper config value using the template syntax
///
/// ```toml
/// [[steps]]
/// script = "main.js"
/// config.device = "${mapper.device.id}"
/// ```
pub trait MapperParams: 'static + Send + Sync {
    fn get_value(&self, key: &str) -> Option<String>;
}

impl MapperParams for HashMap<String, String> {
    fn get_value(&self, key: &str) -> Option<String> {
        self.get(key).map(|value| value.to_owned())
    }
}

pub fn empty_mapper_params() -> Box<dyn MapperParams> {
    Box::new(HashMap::default())
}

/// The params.toml is an optional file, that can be created by the user to customize flows
/// without modifying the original flow definition or JavaScript code.
///
/// The params.toml is a simple list of named values
///
/// ```toml
/// x = { y: 42, z: "hello" }
/// debug = true
/// ```
///
/// A flow definition can reference a parameter value using the template syntax
///
/// ```toml
/// [[steps]]
/// script = "main.js"
/// config.y = "${params.x.y}"
/// config.debug = "${params.debug}"
/// ```
pub struct Params<T> {
    mapper: T,
    params: Map<String, Value>,
}

pub fn params_filename() -> &'static str {
    "params.toml"
}

pub fn params_template_filename() -> &'static str {
    "params.toml.template"
}

pub fn is_params_file(path: &Utf8Path) -> bool {
    let name = path.file_name();
    name == Some(params_filename()) || name == Some(params_template_filename())
}

impl<'a, T: MapperParams + ?Sized> Params<&'a T> {
    pub fn new(mapper_config: &'a T) -> Self {
        Params {
            mapper: mapper_config,
            params: Map::default(),
        }
    }

    /// Load the `params.toml` attached to the flow with the given path
    ///
    /// If there is a `params.toml.template` this file is read first and is used for params default values.
    /// To avoid breaking flows, an ill-formed params.toml file is read as an empty set of params.
    pub async fn load_flow_params(
        mapper_config: &'a T,
        path: &Utf8Path,
    ) -> Result<Self, LoadError> {
        let Some(directory) = path.parent() else {
            return Ok(Params::new(mapper_config));
        };
        let default_params =
            Self::load_params(mapper_config, &directory.join(params_template_filename()))
                .await
                .map_err(|err| log::warn!("{err:?}"))
                .unwrap_or_else(|()| Params::new(mapper_config));
        let user_params = Self::load_params(mapper_config, &directory.join(params_filename()))
            .await
            .map_err(|err| log::warn!("{err:?}"))
            .unwrap_or_else(|()| Params::new(mapper_config));
        Ok(default_params.merge(user_params))
    }

    pub async fn load_params(mapper_config: &'a T, path: &Utf8Path) -> Result<Self, LoadError> {
        if let Ok(true) = tokio::fs::try_exists(path).await {
            let content = read_to_string(&path)
                .await
                .map_err(|err| LoadError::from_io(err, path))?;
            Self::load_toml(mapper_config, &content)
        } else {
            Ok(Params::new(mapper_config))
        }
    }

    pub fn load_toml(mapper_config: &'a T, content: &str) -> Result<Self, LoadError> {
        let params = toml::from_str(content)?;
        Ok(Params {
            mapper: mapper_config,
            params,
        })
    }

    pub fn merge(mut self, other: Self) -> Self {
        self.params.extend(other.params);
        self
    }

    pub fn substitute_all(
        &self,
        values: &Map<String, Value>,
    ) -> Result<Map<String, Value>, LoadError> {
        values
            .iter()
            .map(|(key, value)| match self.substitute(value) {
                Ok(value) => Ok((key.to_string(), value)),
                Err(err) => Err(err),
            })
            .collect()
    }

    /// Substitute all the path expressions of the input value
    pub fn substitute(&self, value: &Value) -> Result<Value, LoadError> {
        match value {
            // To avoid breaking flows, a null value is substituted for an ill-formed params path
            Value::String(expr) => self
                .substitute_path(expr)
                .map_err(|err| log::warn!("{err:?}"))
                .or(Ok(Value::Null)),

            Value::Array(values) => values.iter().map(|value| self.substitute(value)).collect(),

            Value::Object(values) => self.substitute_all(values).map(Value::Object),

            _ => Ok(value.clone()),
        }
    }

    /// Substitute the path expression for its params value
    ///
    /// Return the path as a JSON string if not a template path.
    /// Raise an error if no value is attached to the template path.
    pub fn substitute_path(&self, expr: &str) -> Result<Value, LoadError> {
        let Some(path) = expr
            .trim()
            .strip_prefix("${")
            .and_then(|path| path.strip_suffix("}"))
            .map(|path| path.trim())
        else {
            return Ok(self.substitute_inner_paths(expr).into());
        };

        if let Some(path) = path.strip_prefix("params") {
            match Self::get(&self.params, path.strip_prefix('.')) {
                Ok(value) => Ok(value),
                Err(unknown_path) => Err(LoadError::UnknownParam {
                    path: format!("params.{unknown_path}",),
                }),
            }
        } else if let Some(path) = path.strip_prefix("mapper.") {
            match self.mapper.get_value(path) {
                Some(value) => Ok(value.into()),
                None => Err(LoadError::UnknownParam {
                    path: format!("mapper.{path}",),
                }),
            }
        } else {
            Err(LoadError::UnknownParam {
                path: path.to_string(),
            })
        }
    }

    /// Substitute param values for the path expressions of an input string
    ///
    /// Compared to substitute_path which returns JSON value,
    /// this method operate on strings. The path expressions of the input are replaced
    /// by the string value reference by the path expressions.
    ///
    /// If a path reference a parameter value that is not a string,
    /// the string representation of that value is inserted.
    ///
    /// If a path reference no known parameter, the string "null" is used the replacement string.
    pub fn substitute_inner_paths(&self, input: &str) -> String {
        input
            .split_inclusive('}')
            .flat_map(|s| match s.find("${") {
                None => vec![s.to_string()],
                Some(i) => {
                    let (prefix, expr) = s.split_at(i);
                    let value = self
                        .substitute_path(expr)
                        .map_err(|err| log::warn!("{err:?}"))
                        .unwrap_or(Value::Null);
                    let value_str = match value {
                        Value::String(s) => s.to_string(),
                        _ => value.to_string(),
                    };
                    vec![prefix.to_string(), value_str]
                }
            })
            .collect()
    }

    fn get(values: &Map<String, Value>, steps: Option<&str>) -> Result<Value, String> {
        let Some(path) = steps else {
            return Ok(values.clone().into());
        };
        let (key, next_steps) = match path.split_once(".") {
            None => (path, None),
            Some((key, inner_path)) => (key, Some(inner_path)),
        };
        let Some(value) = values.get(key) else {
            return Err(key.to_string());
        };
        if next_steps.is_none() {
            return Ok(value.clone());
        }
        let Some(inner_values) = value.as_object() else {
            return Err(format!("{key}.*"));
        };
        match Self::get(inner_values, next_steps) {
            Ok(value) => Ok(value),
            Err(unknown_path) => Err(format!("{key}.{unknown_path}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_defaults() {
        let mapper_config = HashMap::new();
        let default_params = Params::load_toml(
            &mapper_config,
            r#"
            x = 1
            y = 2
        "#,
        )
        .unwrap();
        let user_params = Params::load_toml(
            &mapper_config,
            r#"
            y = 42
            z = 3
        "#,
        )
        .unwrap();

        let params = default_params.merge(user_params);

        for (expr, value) in [
            ("${params.x}", json!(1)),
            ("${params.y}", json!(42)),
            ("${params.z}", json!(3)),
        ] {
            assert_eq!(params.substitute_path(expr).unwrap(), value);
        }
    }

    #[test]
    fn substitute_path() {
        let mapper_config = mapper_config();
        let params = Params::load_toml(
            &mapper_config,
            r#"
        x = 42
        y = { a = "foo", b = "bar" }
        z = [1,2,3]
        "#,
        )
        .unwrap();

        for (expr, value) in [
            ("${mapper.device.id}", json!("raspberry-test")),
            ("${params.x}", json!(42)),
            ("${params.y.a}", json!("foo")),
            ("${params.z}", json!([1, 2, 3])),
            (
                "${params}",
                json!({"x": 42, "y": {"a": "foo", "b": "bar"}, "z": [1,2,3]}),
            ),
            ("hello", json!("hello")),
            (
                "x = ${params.x} and y = ${params.y.a}",
                json!("x = 42 and y = foo"),
            ),
        ] {
            assert_eq!(params.substitute_path(expr).unwrap(), value);
        }
    }

    #[test]
    fn substitute_inner_paths() {
        let mapper_config = mapper_config();
        let params = Params::load_toml(
            &mapper_config,
            r#"
        x = 42
        y = { a = "foo", b = "bar" }
        z = [1,2,3]
        "#,
        )
        .unwrap();

        for (expr, value) in [
            (
                "x = ${params.x} and y = ${params.y.a}",
                "x = 42 and y = foo",
            ),
            ("-- ${mapper.device.id} --", "-- raspberry-test --"),
            ("-- ${params.x} --", "-- 42 --"),
            ("-- ${params.y.a} --", "-- foo --"),
            ("-- ${params.z} --", "-- [1,2,3] --"),
            ("-- ${params.y} --", r#"-- {"a":"foo","b":"bar"} --"#),
            ("-- ${params.unknown} --", r#"-- null --"#),
        ] {
            assert_eq!(params.substitute_inner_paths(expr).as_str(), value);
        }
    }

    #[test]
    fn substitute_errors() {
        let mapper_config = mapper_config();
        let params = Params::load_toml(
            &mapper_config,
            r#"
        x = 42
        y = { a = "foo", b = "bar" }
        z = [1,2,3]
        "#,
        )
        .unwrap();

        for (expr, unknown_path) in [
            ("${mapper.foo.bar}", "mapper.foo.bar"),
            ("${config}", "config"),
            ("${params.foo}", "params.foo"),
            ("${params.foo.a}", "params.foo"),
            ("${params.z.a}", "params.z.*"),
        ] {
            let LoadError::UnknownParam { path } = params.substitute_path(expr).unwrap_err() else {
                panic!("An error is expected");
            };
            assert_eq!(path, unknown_path);
        }
    }

    #[test]
    fn substitute() {
        let mapper_config = mapper_config();
        let params = Params::load_toml(
            &mapper_config,
            r#"
        x = 42
        y = { a = "foo", b = "bar" }
        z = [1,2,3]
        "#,
        )
        .unwrap();

        let config = json!({
            "x": "${params.x}",
            "y": "${params.y}",
            "z": "unchanged",
            "source": "${mapper.device.id}",
            "unknown": "${params.unknown}"
        });

        let expected = json!({
            "x": 42,
            "y": { "a": "foo", "b": "bar" },
            "z": "unchanged",
            "source": "raspberry-test",
            "unknown": null
        });

        assert_eq!(params.substitute(&config).unwrap(), expected);
    }

    fn mapper_config() -> impl MapperParams {
        let mut mapper_config = HashMap::new();

        let (key, value) = ("device.id", "raspberry-test");
        mapper_config.insert(key.to_string(), value.to_string());

        mapper_config
    }
}
