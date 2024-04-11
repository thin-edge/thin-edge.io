use crate::mqtt_topics::Channel;
use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::MqttSchema;
use crate::mqtt_topics::OperationType;
use crate::workflow::CommandId;
use crate::workflow::ExitHandlers;
use crate::workflow::OperationName;
use crate::workflow::StateExcerptError;
use crate::workflow::TopicName;
use crate::workflow::WorkflowExecutionError;
use mqtt_channel::MqttMessage;
use mqtt_channel::QoS::AtLeastOnce;
use mqtt_channel::Topic;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;

/// Generic command state that can be used to manipulate any type of command payload.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct GenericCommandState {
    pub topic: Topic,
    pub status: String,
    pub payload: Value,
}

/// Update for a command state
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct GenericStateUpdate {
    pub status: String,
    pub reason: Option<String>,
}

const STATUS: &str = "status";
const INIT: &str = "init";
const SUCCESSFUL: &str = "successful";
const FAILED: &str = "failed";
const REASON: &str = "reason";

impl GenericCommandState {
    /// Create an init state for a sub-operation
    pub fn sub_command_init_state(
        schema: &MqttSchema,
        entity: &EntityTopicId,
        operation: OperationType,
        cmd_id: CommandId,
        sub_operation: OperationName,
    ) -> GenericCommandState {
        let sub_cmd_id = sub_command_id(&operation.to_string(), &cmd_id);
        let topic = schema.topic_for(
            entity,
            &Channel::Command {
                operation: OperationType::Custom(sub_operation),
                cmd_id: sub_cmd_id,
            },
        );
        let status = INIT.to_string();
        let payload = json!({
            STATUS: status
        });

        GenericCommandState {
            topic,
            status,
            payload,
        }
    }

    /// Extract a command state from a json payload
    pub fn from_command_message(message: &MqttMessage) -> Result<Self, WorkflowExecutionError> {
        let topic = message.topic.clone();
        let payload = message.payload_bytes();
        if payload.is_empty() {
            return Ok(GenericCommandState {
                topic,
                status: "".to_string(),
                payload: json!(null),
            });
        }
        let json: Value = serde_json::from_slice(payload)?;
        let status = GenericCommandState::extract_text_property(&json, STATUS)
            .ok_or(WorkflowExecutionError::MissingStatus)?;
        Ok(GenericCommandState {
            topic,
            status: status.to_string(),
            payload: json,
        })
    }

    /// Build an MQTT message to publish the command state
    pub fn into_message(mut self) -> MqttMessage {
        if self.is_cleared() {
            return self.clear_message();
        }
        GenericCommandState::inject_text_property(&mut self.payload, "status", &self.status);
        let topic = &self.topic;
        let payload = self.payload.to_string();
        MqttMessage::new(topic, payload)
            .with_retain()
            .with_qos(AtLeastOnce)
    }

    /// Build an MQTT message to clear the command state
    pub fn clear_message(&self) -> MqttMessage {
        let topic = &self.topic;
        MqttMessage::new(topic, "")
            .with_retain()
            .with_qos(AtLeastOnce)
    }

    /// Update this state
    pub fn update(mut self, update: GenericStateUpdate) -> Self {
        let status = update.status;
        GenericCommandState::inject_text_property(&mut self.payload, STATUS, &status);
        if let Some(reason) = &update.reason {
            GenericCommandState::inject_text_property(&mut self.payload, REASON, reason)
        };

        GenericCommandState { status, ..self }
    }

    /// Inject a json payload into this one
    pub fn update_with_json(mut self, json: Value) -> Self {
        if let (Some(values), Some(new_values)) = (self.payload.as_object_mut(), json.as_object()) {
            for (k, v) in new_values {
                values.insert(k.to_string(), v.clone());
            }
        }
        match GenericCommandState::extract_text_property(&self.payload, STATUS) {
            None => self.fail_with("Unknown status".to_string()),
            Some(status) => GenericCommandState {
                status: status.to_string(),
                ..self
            },
        }
    }

    /// Update the command state with the outcome of a script
    pub fn update_with_script_output(
        self,
        script: String,
        output: std::io::Result<std::process::Output>,
        handlers: ExitHandlers,
    ) -> Self {
        let json_update = handlers.state_update(&script, output);
        self.update_with_json(json_update)
    }

    /// Update the command state with a new status describing the next state
    pub fn move_to(mut self, status: String) -> Self {
        GenericCommandState::inject_text_property(&mut self.payload, STATUS, &status);

        GenericCommandState { status, ..self }
    }

    /// Update the command state to failed status with the given reason
    pub fn fail_with(mut self, reason: String) -> Self {
        let status = FAILED;
        GenericCommandState::inject_text_property(&mut self.payload, STATUS, status);
        GenericCommandState::inject_text_property(&mut self.payload, REASON, &reason);

        GenericCommandState {
            status: status.to_owned(),
            ..self
        }
    }

    /// Mark the command as completed
    pub fn clear(self) -> Self {
        GenericCommandState {
            status: "".to_string(),
            payload: json!(null),
            ..self
        }
    }

    /// Return the error reason if any
    pub fn failure_reason(&self) -> Option<&str> {
        GenericCommandState::extract_text_property(&self.payload, REASON)
    }

    /// Extract a text property from a Json object
    fn extract_text_property<'a>(json: &'a Value, property: &str) -> Option<&'a str> {
        json.as_object()
            .and_then(|o| o.get(property))
            .and_then(|v| v.as_str())
    }

    /// Inject a text property into a Json object
    fn inject_text_property(json: &mut Value, property: &str, value: &str) {
        if let Some(o) = json.as_object_mut() {
            o.insert(property.to_string(), value.into());
        }
    }

    /// Inject values extracted from the message payload into a script command line.
    ///
    /// - The script command is first tokenized using shell escaping rules.
    ///   `/some/script.sh arg1 "arg 2" "arg 3"` -> ["/some/script.sh", "arg1", "arg 2", "arg 3"]
    /// - Then each token matching `${x.y.z}` is substituted with the value pointed by the JSON path.
    pub fn inject_parameters(&self, args: &[String]) -> Vec<String> {
        args.iter().map(|arg| self.inject_parameter(arg)).collect()
    }

    /// Inject values extracted from the message payload into a script argument
    ///
    /// `${.payload}` -> the whole message payload
    /// `${.payload.x}` -> the value of x if there is any in the payload
    /// `${.payload.unknown}` -> `${.payload.unknown}` unchanged
    /// `Not a path expression` -> `Not a path expression` unchanged
    pub fn inject_parameter(&self, script_parameter: &str) -> String {
        Self::extract_path(script_parameter)
            .and_then(|path| self.extract_value(path))
            .map(|v| json_as_string(&v))
            .unwrap_or_else(|| script_parameter.to_string())
    }

    /// Extract a path  from a `${ ... }` expression
    ///
    /// Return None if the input is not a path expression
    pub fn extract_path(input: &str) -> Option<&str> {
        input.strip_prefix("${").and_then(|s| s.strip_suffix('}'))
    }

    /// Extract the JSON value pointed by a path from this command state
    ///
    /// Return None if the path contains unknown fields.
    pub fn extract_value(&self, path: &str) -> Option<Value> {
        match path {
            "." => Some(json!({
                "topic": self.topic.name,
                "payload": self.payload
            })),
            ".topic" => Some(self.topic.name.clone().into()),
            ".topic.target" => self.target().map(|s| s.into()),
            ".topic.operation" => self.operation().map(|s| s.into()),
            ".topic.cmd_id" => self.cmd_id().map(|s| s.into()),
            ".payload" => Some(self.payload.clone()),
            path => path
                .strip_prefix(".payload.")
                .and_then(|path| json_excerpt(&self.payload, path))
                .cloned(),
        }
    }

    /// Return the topic that uniquely identifies the command
    pub fn command_topic(&self) -> &String {
        &self.topic.name
    }

    fn target(&self) -> Option<String> {
        match self.topic.name.split('/').collect::<Vec<&str>>()[..] {
            [_, t1, t2, t3, t4, "cmd", _, _] => Some(format!("{t1}/{t2}/{t3}/{t4}")),
            _ => None,
        }
    }

    pub fn operation(&self) -> Option<String> {
        extract_command_identifier(&self.topic.name).map(|(operation, _)| operation)
    }

    pub fn cmd_id(&self) -> Option<String> {
        extract_command_identifier(&self.topic.name).map(|(_, cmd_id)| cmd_id)
    }

    pub fn is_init(&self) -> bool {
        matches!(self.status.as_str(), INIT)
    }

    pub fn is_successful(&self) -> bool {
        matches!(self.status.as_str(), SUCCESSFUL)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self.status.as_str(), FAILED)
    }

    pub fn is_cleared(&self) -> bool {
        self.payload.is_null()
    }
}

/// Return the invoking command topic name, if any
pub fn invoking_command(sub_command: &TopicName) -> Option<TopicName> {
    match sub_command.split('/').collect::<Vec<&str>>()[..] {
        [pre, t1, t2, t3, t4, "cmd", _, sub_id] => extract_invoking_command_id(sub_id)
            .map(|(op, id)| format!("{pre}/{t1}/{t2}/{t3}/{t4}/cmd/{op}/{id}")),
        _ => None,
    }
}

/// Build a sub command identifier from its invoking command identifier
///
/// Using such a structure command id for sub commands is key
/// to retrieve the invoking command of a sub-operation from its state using [extract_invoking_command_id].
fn sub_command_id(operation: &str, cmd_id: &str) -> String {
    format!("sub:{operation}:{cmd_id}")
}

/// Extract the invoking command identifier from a sub command identifier
///
/// Return None if the given id is not a sub command identifier
/// i.e. if not generated with [sub_command_id].
fn extract_invoking_command_id(sub_cmd_id: &str) -> Option<(&str, &str)> {
    match sub_cmd_id.split(':').collect::<Vec<&str>>()[..] {
        ["sub", operation, cmd_id] => Some((operation, cmd_id)),
        _ => None,
    }
}

pub fn extract_command_identifier(topic: &str) -> Option<(String, String)> {
    match topic.split('/').collect::<Vec<&str>>()[..] {
        [_, _, _, _, _, "cmd", operation, cmd_id] => {
            Some((operation.to_string(), cmd_id.to_string()))
        }
        _ => None,
    }
}

impl GenericStateUpdate {
    pub fn empty_payload() -> Value {
        json!({})
    }

    pub fn init_payload() -> Value {
        json!({STATUS: INIT})
    }

    pub fn successful() -> Self {
        GenericStateUpdate {
            status: SUCCESSFUL.to_string(),
            reason: None,
        }
    }

    pub fn failed(reason: String) -> Self {
        GenericStateUpdate {
            status: FAILED.to_string(),
            reason: Some(reason),
        }
    }

    pub fn timeout() -> Self {
        Self::failed("timeout".to_string())
    }

    pub fn into_json(self) -> Value {
        self.into()
    }

    /// Inject this state update into a given JSON representing the state update returned by a script.
    ///
    /// - The status field of self always trumps the status field contained by the JSON value (if any).
    /// - The error field of self acts only as a default
    ///   and is injected only no such field is provided by the JSON value.
    pub fn inject_into_json(self, mut json: Value) -> Value {
        match json.as_object_mut() {
            None => self.into_json(),
            Some(object) => {
                object.insert(STATUS.to_string(), self.status.into());
                if object.get(REASON).is_none() {
                    if let Some(reason) = self.reason {
                        object.insert(REASON.to_string(), reason.into());
                    }
                }
                json
            }
        }
    }
}

impl Default for GenericStateUpdate {
    fn default() -> Self {
        GenericStateUpdate::successful()
    }
}

impl From<String> for GenericStateUpdate {
    fn from(status: String) -> Self {
        GenericStateUpdate {
            status,
            reason: None,
        }
    }
}

impl From<GenericStateUpdate> for Value {
    fn from(update: GenericStateUpdate) -> Self {
        match update.reason {
            None => json!({
                STATUS: update.status
            }),
            Some(reason) => json!({
                STATUS: update.status,
                REASON: reason,
            }),
        }
    }
}

fn json_excerpt<'a>(value: &'a Value, path: &'a str) -> Option<&'a Value> {
    match path.split_once('.') {
        None if path.is_empty() => Some(value),
        None => value.get(path),
        Some((key, path)) => value.get(key).and_then(|value| json_excerpt(value, path)),
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

/// A set of values to be injected/extracted into/from a [GenericCommandState]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(try_from = "Option<Value>")]
pub enum StateExcerpt {
    /// A constant JSON value
    Literal(Value),

    /// A JSON path to the excerpt
    ///
    /// `"${.some.value.extracted.from.a.command.state}"`
    PathExpr(String),

    /// A map of named excerpts
    ///
    /// `{ x = "${.some.x.value}", y = "${.some.y.value}"`
    ExcerptMap(HashMap<String, StateExcerpt>),

    /// An array of excerpts
    ///
    /// `["${.some.x.value}", "${.some.y.value}"]`
    ExcerptArray(Vec<StateExcerpt>),
}

impl StateExcerpt {
    /// Extract a JSON value from the input state
    pub fn extract_value_from(&self, input: &GenericCommandState) -> Value {
        match self {
            StateExcerpt::Literal(value) => value.clone(),
            StateExcerpt::PathExpr(path) => input.extract_value(path).unwrap_or(Value::Null),
            StateExcerpt::ExcerptMap(excerpts) => {
                let mut values = serde_json::Map::new();
                for (key, excerpt) in excerpts {
                    let value = excerpt.extract_value_from(input);
                    values.insert(key.to_string(), value);
                }
                Value::Object(values)
            }
            StateExcerpt::ExcerptArray(excerpts) => {
                let mut values = Vec::new();
                for excerpt in excerpts {
                    let value = excerpt.extract_value_from(input);
                    values.push(value);
                }
                Value::Array(values)
            }
        }
    }
}

impl TryFrom<Option<Value>> for StateExcerpt {
    type Error = StateExcerptError;

    fn try_from(value: Option<Value>) -> Result<Self, Self::Error> {
        match value {
            None | Some(Value::Null) => {
                // A mapping that change nothing
                Ok(StateExcerpt::ExcerptMap(HashMap::new()))
            }
            Some(value) if value.is_object() => Ok(value.into()),
            Some(value) => {
                let kind = match &value {
                    Value::Bool(_) => "bool",
                    Value::Number(_) => "number",
                    Value::String(_) => "string",
                    Value::Array(_) => "array",
                    _ => unreachable!(),
                };
                Err(StateExcerptError::NotAnObject {
                    kind: kind.to_string(),
                    value,
                })
            }
        }
    }
}

impl From<Value> for StateExcerpt {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => StateExcerpt::Literal(Value::Null),
            Value::Bool(b) => StateExcerpt::Literal(Value::Bool(b)),
            Value::Number(n) => StateExcerpt::Literal(Value::Number(n)),
            Value::String(s) => match GenericCommandState::extract_path(&s) {
                None => StateExcerpt::Literal(Value::String(s)),
                Some(path) => StateExcerpt::PathExpr(path.to_string()),
            },
            Value::Array(a) => {
                StateExcerpt::ExcerptArray(a.iter().map(|v| v.clone().into()).collect())
            }
            Value::Object(o) => StateExcerpt::ExcerptMap(
                o.iter()
                    .map(|(k, v)| (k.to_owned(), v.clone().into()))
                    .collect(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_channel::Topic;
    use serde_json::json;

    #[test]
    fn serde_generic_command_payload() {
        let topic = Topic::new_unchecked("te/device/main///cmd/make_it/123");
        let payload = r#"{ "status":"init", "foo":42, "bar": { "extra": [1,2,3] }}"#;
        let command = mqtt_channel::MqttMessage::new(&topic, payload);
        let cmd = GenericCommandState::from_command_message(&command).expect("parsing error");
        assert!(cmd.is_init());
        assert_eq!(
            cmd,
            GenericCommandState {
                topic: topic.clone(),
                status: "init".to_string(),
                payload: json!({
                    "status": "init",
                    "foo": 42,
                    "bar": {
                        "extra": [1,2,3]
                    }
                })
            }
        );

        let update_cmd = cmd.move_to("executing".to_string());
        assert_eq!(
            update_cmd,
            GenericCommandState {
                topic: topic.clone(),
                status: "executing".to_string(),
                payload: json!({
                    "status": "executing",
                    "foo": 42,
                    "bar": {
                        "extra": [1,2,3]
                    }
                })
            }
        );

        let final_cmd = update_cmd.fail_with("panic".to_string());
        assert_eq!(
            final_cmd,
            GenericCommandState {
                topic: topic.clone(),
                status: "failed".to_string(),
                payload: json!({
                    "status": "failed",
                    "reason": "panic",
                    "foo": 42,
                    "bar": {
                        "extra": [1,2,3]
                    }
                })
            }
        );
    }

    #[test]
    fn inject_json_into_parameters() {
        let topic = Topic::new_unchecked("te/device/main///cmd/make_it/123");
        let payload = r#"{ "status":"init", "foo":42, "bar": { "extra": [1,2,3] }}"#;
        let command = mqtt_channel::MqttMessage::new(&topic, payload);
        let cmd = GenericCommandState::from_command_message(&command).expect("parsing error");
        assert!(cmd.is_init());

        // Valid paths
        assert_eq!(
            cmd.inject_parameter("${.}").to_json(),
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
            cmd.inject_parameter("${.topic}"),
            "te/device/main///cmd/make_it/123"
        );
        assert_eq!(cmd.inject_parameter("${.topic.target}"), "device/main//");
        assert_eq!(cmd.inject_parameter("${.topic.operation}"), "make_it");
        assert_eq!(cmd.inject_parameter("${.topic.cmd_id}"), "123");
        assert_eq!(cmd.inject_parameter("${.payload}").to_json(), cmd.payload);
        assert_eq!(cmd.inject_parameter("${.payload.status}"), "init");
        assert_eq!(cmd.inject_parameter("${.payload.foo}"), "42");
        assert_eq!(
            cmd.inject_parameter("${.payload.bar}").to_json(),
            json!({
                "extra": [1,2,3]
            })
        );
        assert_eq!(
            cmd.inject_parameter("${.payload.bar.extra}").to_json(),
            json!([1, 2, 3])
        );

        // Not supported yet
        assert_eq!(
            cmd.inject_parameter("${.payload.bar.extra[1]}"),
            "${.payload.bar.extra[1]}"
        );

        // Ill formed
        assert_eq!(cmd.inject_parameter("not a pattern"), "not a pattern");
        assert_eq!(cmd.inject_parameter("${ill-formed}"), "${ill-formed}");
        assert_eq!(cmd.inject_parameter("${.unknown}"), "${.unknown}");
        assert_eq!(
            cmd.inject_parameter("${.payload.bar.unknown}"),
            "${.payload.bar.unknown}"
        );
    }

    #[test]
    fn parse_empty_payload() {
        let topic = Topic::new_unchecked("te/device/main///cmd/make_it/123");
        let command = mqtt_channel::MqttMessage::new(&topic, "".to_string());
        let cmd = GenericCommandState::from_command_message(&command).expect("parsing error");
        assert!(cmd.is_cleared())
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
