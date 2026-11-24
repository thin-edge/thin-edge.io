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
        template_segments(target)
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
        extract_path(input)
    }
}

impl Record for GenericCommandState {
    /// Extract the JSON value pointed by a path from this command state
    ///
    /// Return None if the path contains unknown fields,
    /// with the exception that the empty string is returned for an unknown path below the `.payload`,
    /// the rational being that the payload object represents a free-form value.
    /// An unknown path below the `.entity` also returns the empty string, when the command holds entity data.
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
            path if is_entity_path(path) => {
                let value = extract_entity_value(self.entity_data()?, path)
                    .cloned()
                    .unwrap_or_else(|| String::new().into());
                Some(value)
            }
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
    if path.is_empty() {
        return Some(value);
    }

    match path.split_once('.') {
        None => json_array_excerpt(value, path),
        Some((key, path)) => {
            json_array_excerpt(value, key).and_then(|value| json_excerpt(value, path))
        }
    }
}

fn json_array_excerpt<'a>(value: &'a Value, path: &'a str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }

    match path.find('[') {
        None => value.get(path),
        Some(0) => {
            let (key, path) = path[1..].split_once(']')?;
            let index = key.parse::<i64>().ok()?;
            let items = value.as_array()?;
            let abs_index = if index >= 0 {
                index
            } else {
                (items.len() as i64) + index
            };
            items
                .get(abs_index as usize)
                .and_then(|value| json_array_excerpt(value, path))
        }
        Some(pos) => {
            let (key, path) = path.split_at(pos);
            value
                .get(key)
                .and_then(|value| json_array_excerpt(value, path))
        }
    }
}

const ENTITY: &str = ".entity";
const ENTITY_PREFIX: &str = ".entity.";

pub(crate) fn is_entity_path(path: &str) -> bool {
    path == ENTITY || path.starts_with(ENTITY_PREFIX)
}

/// Returns `true` when `template` uses a `${.entity.*}` expression
pub(crate) fn uses_entity_data(template: &str) -> bool {
    template_segments(template)
        .filter_map(extract_path)
        .any(is_entity_path)
}

/// Splits a template into plain text and `${...}` expressions
fn template_segments(template: &str) -> impl Iterator<Item = &str> {
    template
        .split_inclusive('}')
        .flat_map(|s| match s.find("${") {
            None => vec![s],
            Some(i) => {
                let (prefix, expression) = s.split_at(i);
                vec![prefix, expression]
            }
        })
}

fn extract_entity_value<'a>(entity: &'a Value, path: &'a str) -> Option<&'a Value> {
    match path {
        ENTITY => Some(entity),
        path => json_excerpt(entity, path.strip_prefix(ENTITY_PREFIX)?),
    }
}

fn extract_path(input: &str) -> Option<&str> {
    input.strip_prefix("${").and_then(|s| s.strip_suffix('}'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityMetadata;
    use crate::entity::EntityType;
    use crate::mqtt_topics::EntityTopicId;
    use crate::workflow::GenericCommandState;
    use mqtt_channel::Topic;
    use serde_json::json;
    use serde_json::Map;

    #[test]
    fn inject_json_into_parameters() {
        let topic = Topic::new_unchecked("te/device/main///cmd/make_it/123");
        let payload = json!({
            "status":"init",
            "foo":42,
            "bar": { "extra": [1,2,3] },
            "nested": [ { "matrix": [[4,5,6], [7,8,9]]} ]
        });
        let command = mqtt_channel::MqttMessage::new(&topic, payload.to_string());
        let cmd = GenericCommandState::from_command_message(&command).expect("parsing error");
        assert!(cmd.is_init());

        // Valid paths
        assert_eq!(
            cmd.inject_values_into_template("${.}").to_json(),
            json!({
                    "topic": "te/device/main///cmd/make_it/123",
                    "payload": payload,
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
        assert_eq!(
            cmd.inject_values_into_template("${.payload.bar.extra[1]}")
                .to_json(),
            json!(2)
        );
        assert_eq!(
            cmd.inject_values_into_template("${.payload.bar.extra[3]}"),
            ""
        );
        assert_eq!(
            cmd.inject_values_into_template("${.payload.bar.extra[-1]}")
                .to_json(),
            json!(3)
        );
        assert_eq!(
            cmd.inject_values_into_template("${.payload.bar.extra[-3]}")
                .to_json(),
            json!(1)
        );
        assert_eq!(
            cmd.inject_values_into_template("${.payload.bar.extra[-4]}"),
            ""
        );
        assert_eq!(
            cmd.inject_values_into_template("${.payload.nested[0].matrix[1]}")
                .to_json(),
            json!([7, 8, 9])
        );
        assert_eq!(
            cmd.inject_values_into_template("${.payload.nested[0].matrix[1][2]}")
                .to_json(),
            json!(9)
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
        assert_eq!(
            cmd.inject_values_into_template("${.payload.bar[extra]}"), // only integer can be used as indexes
            ""
        );
        assert_eq!(
            cmd.inject_values_into_template("${.payload.bar[0]}"), // not an array
            ""
        );
        assert_eq!(cmd.inject_values_into_template("${.payload.bar[}"), "");
        assert_eq!(cmd.inject_values_into_template("${.payload.bar]}"), "");
    }

    #[test]
    fn inject_entity_data_into_parameters() {
        let cmd = command_with_entity_data();

        assert_eq!(
            cmd.inject_values_into_template("${.entity.topic_id}"),
            "device/main/service/nodered"
        );
        assert_eq!(
            cmd.inject_values_into_template("${.entity.type}"),
            "service"
        );
        assert_eq!(
            cmd.inject_values_into_template("${.entity.parent}"),
            "device/main//"
        );
        assert_eq!(
            cmd.inject_values_into_template("${.entity.twin.name}"),
            "Node-RED"
        );
        assert_eq!(
            cmd.inject_values_into_template("${.entity}").to_json(),
            json!({
                "topic_id": "device/main/service/nodered",
                "type": "service",
                "parent": "device/main//",
                "twin": { "name": "Node-RED" },
            })
        );
    }

    #[test]
    fn a_path_the_entity_has_no_value_for_is_substituted_for_the_empty_string() {
        let cmd = command_with_entity_data();

        assert_eq!(cmd.inject_values_into_template("${.entity.id}"), "");
        assert_eq!(cmd.inject_values_into_template("${.entity.twin.type}"), "");
    }

    #[test]
    fn a_command_with_no_entity_data_leaves_the_template_unresolved() {
        let cmd = command_without_entity_data();

        assert_eq!(
            cmd.inject_values_into_template("${.entity.topic_id}"),
            "${.entity.topic_id}"
        );
    }

    #[test]
    fn the_whole_state_carries_no_entity_data() {
        let cmd = command_with_entity_data();

        assert_eq!(
            cmd.inject_values_into_template("${.}").to_json(),
            json!({
                "topic": "te/device/main/service/nodered/cmd/restart/123",
                "payload": { "status": "init" },
            })
        );
    }

    #[test]
    fn only_a_template_with_an_entity_path_uses_entity_data() {
        assert!(uses_entity_data(
            "tedge service restart ${.entity.twin.name}"
        ));
        assert!(uses_entity_data("${.payload.x}-${.entity.id}"));
        assert!(uses_entity_data("/usr/bin/handle.sh ${.entity}"));
        assert!(!uses_entity_data("/usr/bin/handle.sh ${.payload.x}"));
        assert!(!uses_entity_data("no path at all"));
    }

    fn command_without_entity_data() -> GenericCommandState {
        let topic = Topic::new_unchecked("te/device/main/service/nodered/cmd/restart/123");
        let message = mqtt_channel::MqttMessage::new(&topic, json!({"status": "init"}).to_string());
        GenericCommandState::from_command_message(&message).expect("parsing error")
    }

    fn command_with_entity_data() -> GenericCommandState {
        let entity = EntityMetadata::new(
            "device/main/service/nodered".parse().unwrap(),
            EntityType::Service,
        )
        .with_parent(EntityTopicId::default_main_device());
        let twin = Map::from_iter([("name".to_string(), json!("Node-RED"))]);

        command_without_entity_data().with_entity_data(&entity, twin)
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
