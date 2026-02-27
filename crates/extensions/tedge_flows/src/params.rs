use crate::LoadError;
use camino::Utf8Path;
use serde_json::Map;
use serde_json::Value;
use tokio::fs::read_to_string;

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
/// config.y = "{.params.x.y}"
/// config.debug = "{.params.debug}"
/// ```
#[derive(serde::Deserialize, Default)]
pub struct Params {
    params: Map<String, Value>,
}

impl Params {
    pub fn filename() -> &'static str {
        "params.toml"
    }

    pub fn is_params_file(path: &Utf8Path) -> bool {
        path.file_name() == Some(Params::filename())
    }

    pub async fn load_flow_params(path: &Utf8Path) -> Result<Option<Params>, LoadError> {
        let Some(directory) = path.parent() else {
            return Ok(None);
        };
        Self::load_params(directory).await
    }

    /// Load the `params.toml` used by all the flows of the given directory
    pub async fn load_params(directory: &Utf8Path) -> Result<Option<Params>, LoadError> {
        let path = directory.join(Self::filename());
        if let Ok(true) = tokio::fs::try_exists(&path).await {
            let content = read_to_string(&path)
                .await
                .map_err(|err| LoadError::from_io(err, &path))?;
            Self::load_toml(&content).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn load_toml(content: &str) -> Result<Params, LoadError> {
        let params = toml::from_str(content)?;
        Ok(Params { params })
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
            Value::String(expr) => self.substitute_path(expr).or(Ok(Value::Null)),

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
        else {
            return Ok(expr.into());
        };
        let Some(path) = path.trim().strip_prefix(".params") else {
            return Err(LoadError::UnknownParam {
                path: path.to_string(),
            });
        };
        match Self::get(&self.params, path.strip_prefix('.')) {
            Ok(value) => Ok(value),
            Err(unknown_path) => Err(LoadError::UnknownParam {
                path: format!(".params.{unknown_path}",),
            }),
        }
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
    fn substitute_path() {
        let params = Params::load_toml(
            r#"
        x = 42
        y = { a = "foo", b = "bar" }
        z = [1,2,3]
        "#,
        )
        .unwrap();

        for (expr, value) in [
            ("${.params.x}", json!(42)),
            ("${.params.y.a}", json!("foo")),
            ("${.params.z}", json!([1, 2, 3])),
            (
                "${.params}",
                json!({"x": 42, "y": {"a": "foo", "b": "bar"}, "z": [1,2,3]}),
            ),
            ("hello", json!("hello")),
        ] {
            assert_eq!(params.substitute_path(expr).unwrap(), value);
        }
    }

    #[test]
    fn substitute_errors() {
        let params = Params::load_toml(
            r#"
        x = 42
        y = { a = "foo", b = "bar" }
        z = [1,2,3]
        "#,
        )
        .unwrap();

        for (expr, unknown_path) in [
            ("${.config}", ".config"),
            ("${.params.foo}", ".params.foo"),
            ("${.params.foo.a}", ".params.foo"),
            ("${.params.z.a}", ".params.z.*"),
        ] {
            let LoadError::UnknownParam { path } = params.substitute_path(expr).unwrap_err() else {
                panic!("An error is expected");
            };
            assert_eq!(path, unknown_path);
        }
    }

    #[test]
    fn substitute() {
        let params = Params::load_toml(
            r#"
        x = 42
        y = { a = "foo", b = "bar" }
        z = [1,2,3]
        "#,
        )
        .unwrap();

        let config = json!({
            "x": "${.params.x}",
            "y": "${.params.y}",
            "z": "unchanged",
            "unknown": "${.params.unknown}"
        });

        let expected = json!({
            "x": 42,
            "y": { "a": "foo", "b": "bar" },
            "z": "unchanged",
            "unknown": null
        });

        assert_eq!(params.substitute(&config).unwrap(), expected);
    }
}
