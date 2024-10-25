use crate::workflow::GenericCommandState;
use serde_json::json;
use serde_json::Value;

pub trait Record {
    /// Extract from this record the JSON value pointed by the given path
    fn extract_value(&self, path: &str) -> Option<Value>;

    /// Inject values extracted from the record into a template string
    ///
    /// - Search the template string for path patterns `${...}`
    /// - Replace all these paths by the value extracted from self using the paths
    ///
    /// `"prefix-${.payload.x}-separator-${.payload.y}-suffix"` is replaced by
    /// `"prefix-X-separator-Y-suffix"` in a context where the payload is `{"x":"X", "y":"Y"}`
    fn inject_values_into_template(&self, target: &str) -> String {
        target
            .split_inclusive('}')
            .flat_map(|s| match s.find("${") {
                None => vec![s],
                Some(i) => {
                    let (prefix, template) = s.split_at(i);
                    vec![prefix, template]
                }
            })
            .map(|s| self.replace_path_with_value(s))
            .collect()
    }

    /// Inject values extracted from the record into a vector of target strings.
    fn inject_values_into_templates(&self, targets: &[String]) -> Vec<String> {
        targets
            .iter()
            .map(|arg| self.inject_values_into_template(arg))
            .collect()
    }

    /// Replace a path pattern with the value extracted from the message payload using that path
    ///
    /// `${.payload}` -> the whole message payload
    /// `${.payload.x}` -> the value of x if there is any in the payload
    /// `${.payload.unknown}` -> `${.payload.unknown}` unchanged
    /// `Not a path expression` -> `Not a path expression` unchanged
    fn replace_path_with_value(&self, template: &str) -> String {
        Self::extract_path(template)
            .and_then(|path| self.extract_value(path))
            .map(|v| json_as_string(&v))
            .unwrap_or_else(|| template.to_string())
    }

    /// Extract a path  from a `${ ... }` expression
    ///
    /// Return None if the input is not a path expression
    fn extract_path(input: &str) -> Option<&str> {
        input.strip_prefix("${").and_then(|s| s.strip_suffix('}'))
    }
}

impl Record for GenericCommandState {
    /// Extract the JSON value pointed by a path from this command state
    ///
    /// Return None if the path contains unknown fields,
    /// with the exception that the empty string is returned for an unknown path below the `.payload`,
    /// the rational being that the payload object represents a free-form value.
    fn extract_value(&self, path: &str) -> Option<Value> {
        match path {
            "." => Some(json!({
                "topic": self.topic.name,
                "payload": self.payload
            })),
            ".topic" => Some(self.topic.name.clone().into()),
            ".topic.root_prefix" => self.root_prefix().map(|s| s.into()),
            ".topic.target" => self.target().map(|s| s.into()),
            ".topic.operation" => self.operation().map(|s| s.into()),
            ".topic.cmd_id" => self.cmd_id().map(|s| s.into()),
            ".payload" => Some(self.payload.clone()),
            path if path.contains(['[', ']']) => None,
            path => {
                let value_path = path.strip_prefix(".payload.")?;
                let value = json_excerpt(&self.payload, value_path)
                    .cloned()
                    .unwrap_or_else(|| String::new().into());
                Some(value)
            }
        }
    }
}

fn json_as_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}

fn json_excerpt<'a>(value: &'a Value, path: &'a str) -> Option<&'a Value> {
    match path.split_once('.') {
        None if path.is_empty() => Some(value),
        None => value.get(path),
        Some((key, path)) => value.get(key).and_then(|value| json_excerpt(value, path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::GenericCommandState;
    use mqtt_channel::Topic;
    use serde_json::json;

    #[test]
    fn inject_json_into_parameters() {
        let topic = Topic::new_unchecked("te/device/main///cmd/make_it/123");
        let payload = r#"{ "status":"init", "foo":42, "bar": { "extra": [1,2,3] }}"#;
        let command = mqtt_channel::MqttMessage::new(&topic, payload);
        let cmd = GenericCommandState::from_command_message(&command).expect("parsing error");
        assert!(cmd.is_init());

        // Valid paths
        assert_eq!(
            cmd.inject_values_into_template("${.}").to_json(),
            json!({
                "topic": "te/device/main///cmd/make_it/123",
                "payload": {
                    "status":"init",
                    "foo":42,
                    "bar": { "extra": [1,2,3] }
                }
            })
        );
        assert_eq!(
            cmd.inject_values_into_template("${.topic}"),
            "te/device/main///cmd/make_it/123"
        );
        assert_eq!(
            cmd.inject_values_into_template("${.topic.target}"),
            "device/main//"
        );
        assert_eq!(
            cmd.inject_values_into_template("${.topic.operation}"),
            "make_it"
        );
        assert_eq!(cmd.inject_values_into_template("${.topic.cmd_id}"), "123");
        assert_eq!(
            cmd.inject_values_into_template("${.payload}").to_json(),
            cmd.payload
        );
        assert_eq!(
            cmd.inject_values_into_template("${.payload.status}"),
            "init"
        );
        assert_eq!(cmd.inject_values_into_template("${.payload.foo}"), "42");
        assert_eq!(
            cmd.inject_values_into_template("prefix-${.payload.foo}"),
            "prefix-42"
        );
        assert_eq!(
            cmd.inject_values_into_template("${.payload.foo}-suffix"),
            "42-suffix"
        );
        assert_eq!(
            cmd.inject_values_into_template("prefix-${.payload.foo}-suffix"),
            "prefix-42-suffix"
        );
        assert_eq!(
            cmd.inject_values_into_template(
                "prefix-${.payload.foo}-separator-${.topic.cmd_id}-suffix"
            ),
            "prefix-42-separator-123-suffix"
        );
        assert_eq!(
            cmd.inject_values_into_template("prefix-${.payload.foo}-separator-${invalid-path}"),
            "prefix-42-separator-${invalid-path}"
        );
        assert_eq!(
            cmd.inject_values_into_template("not-a-valid-pattern}"),
            "not-a-valid-pattern}"
        );
        assert_eq!(
            cmd.inject_values_into_template("${not-a-valid-pattern"),
            "${not-a-valid-pattern"
        );
        assert_eq!(
            cmd.inject_values_into_template("${.payload.bar}").to_json(),
            json!({
                "extra": [1,2,3]
            })
        );
        assert_eq!(
            cmd.inject_values_into_template("${.payload.bar.extra}")
                .to_json(),
            json!([1, 2, 3])
        );

        // Not supported yet
        assert_eq!(
            cmd.inject_values_into_template("${.payload.bar.extra[1]}"),
            "${.payload.bar.extra[1]}"
        );

        // Ill formed
        assert_eq!(
            cmd.inject_values_into_template("not a pattern"),
            "not a pattern"
        );
        assert_eq!(
            cmd.inject_values_into_template("${ill-formed}"),
            "${ill-formed}"
        );
        assert_eq!(
            cmd.inject_values_into_template("${.unknown_root}"),
            "${.unknown_root}"
        );
        assert_eq!(
            cmd.inject_values_into_template("${.payload.bar.unknown}"),
            ""
        );
    }

    trait JsonContent {
        fn to_json(self) -> Value;
    }

    impl JsonContent for String {
        fn to_json(self) -> Value {
            match serde_json::from_str(&self) {
                Ok(json) => json,
                Err(_) => Value::Null,
            }
        }
    }
}
