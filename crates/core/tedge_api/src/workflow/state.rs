use crate::mqtt_topics::Channel;
use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::MqttSchema;
use crate::mqtt_topics::OperationType;
use crate::workflow::CommandId;
use crate::workflow::ExitHandlers;
use crate::workflow::OperationName;
use crate::workflow::StateExcerptError;
use crate::workflow::WorkflowExecutionError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use mqtt_channel::MqttMessage;
use mqtt_channel::QoS::AtLeastOnce;
use mqtt_channel::Topic;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Display;

pub const OP_LOG_PATH_KEY: &str = "logPath";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenericCommandData {
    State(GenericCommandState),
    Metadata(GenericCommandMetadata),
}

impl From<GenericCommandState> for GenericCommandData {
    fn from(value: GenericCommandState) -> Self {
        GenericCommandData::State(value)
    }
}

impl From<GenericCommandMetadata> for GenericCommandData {
    fn from(value: GenericCommandMetadata) -> Self {
        GenericCommandData::Metadata(value)
    }
}

/// Generic command metadata.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct GenericCommandMetadata {
    pub operation: OperationName,
    pub payload: Value,
}

/// Generic command state that can be used to manipulate any type of command payload.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct GenericCommandState {
    pub topic: Topic,
    pub status: String,
    pub payload: Value,
    invoking_command_topic: Option<String>,
}

/// Update for a command state
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct GenericStateUpdate {
    pub status: String,
    pub reason: Option<String>,
}

const STATUS: &str = "status";
const INIT: &str = "init";
const SCHEDULED: &str = "scheduled";
const EXECUTING: &str = "executing";
const SUCCESSFUL: &str = "successful";
const FAILED: &str = "failed";
const REASON: &str = "reason";

impl GenericCommandState {
    pub fn new(topic: Topic, status: String, payload: Value) -> Self {
        let invoking_command_topic = Self::infer_invoking_command_topic(topic.as_ref());
        GenericCommandState {
            topic,
            status,
            payload,
            invoking_command_topic,
        }
    }

    /// Create an init state for a sub-operation
    pub fn sub_command_init_state(
        schema: &MqttSchema,
        entity: &EntityTopicId,
        operation: OperationType,
        cmd_id: CommandId,
        sub_operation: OperationName,
    ) -> GenericCommandState {
        let sub_cmd_id = Self::sub_command_id(&operation, &cmd_id);
        let topic = schema.topic_for(
            entity,
            &Channel::Command {
                operation: OperationType::Custom(sub_operation),
                cmd_id: sub_cmd_id,
            },
        );
        let invoking_command_topic =
            schema.topic_for(entity, &Channel::Command { operation, cmd_id });
        let status = INIT.to_string();
        let payload = json!({
            STATUS: status
        });

        GenericCommandState {
            topic,
            status,
            payload,
            invoking_command_topic: Some(invoking_command_topic.name),
        }
    }

    /// Extract a command state from a json payload
    pub fn from_command_message(message: &MqttMessage) -> Result<Self, WorkflowExecutionError> {
        let topic = message.topic.clone();
        let invoking_command_topic = Self::infer_invoking_command_topic(topic.as_ref());
        let bytes = message.payload_bytes();
        let (status, payload) = if bytes.is_empty() {
            ("".to_string(), json!(null))
        } else {
            let json: Value = serde_json::from_slice(bytes)?;
            let status = GenericCommandState::extract_text_property(&json, STATUS)
                .ok_or(WorkflowExecutionError::MissingStatus)?;
            (status.to_string(), json)
        };

        Ok(GenericCommandState {
            topic,
            status,
            payload,
            invoking_command_topic,
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
    fn clear_message(&self) -> MqttMessage {
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

    pub fn update_with_key_value(self, key: &str, val: &str) -> Self {
        self.update_with_json(json!({ key: val }))
    }

    pub fn get_log_path(&self) -> Option<Utf8PathBuf> {
        self.payload
            .get(OP_LOG_PATH_KEY)
            .and_then(|v| v.as_str())
            .map(Utf8PathBuf::from)
    }

    pub fn set_log_path<P: AsRef<Utf8Path>>(self, path: P) -> Self {
        self.update_with_key_value(OP_LOG_PATH_KEY, path.as_ref().as_str())
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

    /// Merge this state into a more complete state overriding all values defined both side
    pub fn merge_into(self, mut state: Self) -> Self {
        state.status = self.status;
        if let Some(properties) = state.payload.as_object_mut() {
            if let Value::Object(new_properties) = self.payload {
                for (key, value) in new_properties.into_iter() {
                    properties.insert(key, value);
                }
            }
        }
        state
    }

    /// Update the command state with a new status describing the next state
    pub fn move_to(mut self, update: GenericStateUpdate) -> Self {
        let status = update.status;
        GenericCommandState::inject_text_property(&mut self.payload, STATUS, &status);

        if let Some(reason) = update.reason {
            GenericCommandState::inject_text_property(&mut self.payload, REASON, &reason);
        }

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
    pub fn inject_values_into_parameters(&self, args: &[String]) -> Vec<String> {
        args.iter()
            .map(|arg| self.inject_values_into_template(arg))
            .collect()
    }

    /// Inject values extracted from the message payload into a template string
    ///
    /// - Search the template string for path patterns `${...}`
    /// - Replace all these paths by the value extracted from self using the paths
    ///
    /// `"prefix-${.payload.x}-separator-${.payload.y}-suffix"` is replaced by
    /// `"prefix-X-separator-Y-suffix"` in a context where the payload is `{"x":"X", "y":"Y"}`
    pub fn inject_values_into_template(&self, target: &str) -> String {
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
    pub fn extract_path(input: &str) -> Option<&str> {
        input.strip_prefix("${").and_then(|s| s.strip_suffix('}'))
    }

    /// Extract the JSON value pointed by a path from this command state
    ///
    /// Return None if the path contains unknown fields,
    /// with the exception that the empty string is returned for an unknown path below the `.payload`,
    /// the rational being that the payload object represents a free-form value.
    pub fn extract_value(&self, path: &str) -> Option<Value> {
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

    /// Return the topic that uniquely identifies the command
    pub fn command_topic(&self) -> &String {
        &self.topic.name
    }

    /// Return the topic of the invoking command, if any
    pub fn invoking_command_topic(&self) -> Option<&str> {
        self.invoking_command_topic.as_deref()
    }

    /// Return the chain of operations leading to this command (excluding the operation itself)
    pub fn invoking_operation_names(&self) -> Vec<String> {
        match self.cmd_id() {
            None => Vec::new(),
            Some(id) => Self::extract_invoking_operation_names(&id),
        }
    }

    /// Infer the topic of the invoking command, given a sub command topic
    fn infer_invoking_command_topic(sub_command_topic: &str) -> Option<String> {
        let schema = MqttSchema::from_topic(sub_command_topic);
        match schema.entity_channel_of(sub_command_topic) {
            Ok((entity, Channel::Command { cmd_id, .. })) => {
                Self::extract_invoking_command_id(&cmd_id).map(|(op, id)| {
                    let channel = Channel::Command {
                        operation: op.into(),
                        cmd_id: id.into(),
                    };
                    schema.topic_for(&entity, &channel).as_ref().to_string()
                })
            }
            _ => None,
        }
    }

    /// Build a sub command identifier from its invoking command identifier
    ///
    /// Using such a structure command id for sub commands is key
    /// to retrieve the invoking command of a sub-operation from its state using [extract_invoking_command_id].
    fn sub_command_id(operation: &impl Display, cmd_id: &impl Display) -> String {
        format!("sub:{operation}:{cmd_id}")
    }

    /// Extract the invoking command identifier from a sub command identifier
    ///
    /// Return None if the given id is not a sub command identifier, i.e. if not generated with [sub_command_id].
    fn extract_invoking_command_id(sub_cmd_id: &str) -> Option<(&str, &str)> {
        sub_cmd_id
            .strip_prefix("sub:")
            .and_then(|op_id| op_id.split_once(':'))
    }

    /// Extract the invoking operation names from a command identifier
    ///
    /// Convert sub:firmware_update:sub:device_profile:robot-123
    /// into ["device_profile", "firmware_update"]
    fn extract_invoking_operation_names(mut cmd_id: &str) -> Vec<String> {
        let mut operations = Vec::new();
        while let Some((op, sub_cmd_id)) = Self::extract_invoking_command_id(cmd_id) {
            operations.push(op.to_string());
            cmd_id = sub_cmd_id;
        }
        operations.reverse();
        operations
    }

    pub fn root_prefix(&self) -> Option<String> {
        MqttSchema::get_root_prefix(&self.topic)
    }

    pub fn target(&self) -> Option<String> {
        MqttSchema::get_entity_id(&self.topic)
    }

    pub fn operation(&self) -> Option<String> {
        MqttSchema::get_operation_name(&self.topic)
    }

    pub fn cmd_id(&self) -> Option<String> {
        MqttSchema::get_command_id(&self.topic)
    }

    pub fn is_init(&self) -> bool {
        self.status.as_str() == INIT
    }

    pub fn is_executing(&self) -> bool {
        self.status.as_str() == EXECUTING
    }

    pub fn is_successful(&self) -> bool {
        self.status.as_str() == SUCCESSFUL
    }

    pub fn is_failed(&self) -> bool {
        self.status.as_str() == FAILED
    }

    pub fn is_finished(&self) -> bool {
        self.is_successful() || self.is_failed()
    }

    pub fn is_cleared(&self) -> bool {
        self.payload.is_null()
    }
}

impl GenericStateUpdate {
    pub fn empty_payload() -> Value {
        json!({})
    }

    pub fn init_payload() -> Value {
        json!({STATUS: INIT})
    }

    pub fn scheduled() -> Self {
        GenericStateUpdate {
            status: SCHEDULED.to_string(),
            reason: None,
        }
    }

    pub fn executing() -> Self {
        GenericStateUpdate {
            status: EXECUTING.to_string(),
            reason: None,
        }
    }

    pub fn successful() -> Self {
        GenericStateUpdate {
            status: SUCCESSFUL.to_string(),
            reason: None,
        }
    }

    pub fn unknown_error() -> Self {
        GenericStateUpdate {
            status: FAILED.to_string(),
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

impl From<&str> for GenericStateUpdate {
    fn from(status: &str) -> Self {
        status.to_string().into()
    }
}

impl Display for GenericStateUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.status.fmt(f)
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
    /// Excerpt returning the whole payload of a command state
    pub fn whole_payload() -> Self {
        StateExcerpt::PathExpr("${.}".to_string())
    }

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
            Some(value) if value.is_object() || value.is_string() => Ok(value.into()),
            Some(value) => {
                let kind = match &value {
                    Value::Bool(_) => "bool",
                    Value::Number(_) => "number",
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
        assert!(!cmd.is_finished());
        assert!(!cmd.is_successful());
        assert!(!cmd.is_failed());
        assert_eq!(cmd.operation(), Some("make_it".to_string()));
        assert!(cmd.invoking_operation_names().is_empty());
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
                }),
                invoking_command_topic: None,
            }
        );

        let update_cmd = cmd.move_to("executing".into());
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
                }),
                invoking_command_topic: None,
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
                }),
                invoking_command_topic: None,
            }
        );
    }

    #[test]
    fn retrieve_invoking_command() {
        let topic = Topic::new_unchecked("te/device/main///cmd/do_it/sub:make_it:456");
        let payload = r#"{ "status":"successful", "foo":42, "bar": { "extra": [1,2,3] }}"#;
        let command = mqtt_channel::MqttMessage::new(&topic, payload);
        let cmd = GenericCommandState::from_command_message(&command).expect("parsing error");
        assert!(cmd.is_successful());
        assert!(cmd.is_finished());
        assert!(!cmd.is_failed());
        assert_eq!(cmd.operation(), Some("do_it".to_string()));
        assert_eq!(cmd.invoking_operation_names(), vec!["make_it".to_string()]);
        assert_eq!(
            cmd,
            GenericCommandState {
                topic: topic.clone(),
                status: "successful".to_string(),
                payload: json!({
                    "status": "successful",
                    "foo": 42,
                    "bar": {
                        "extra": [1,2,3]
                    }
                }),
                invoking_command_topic: Some("te/device/main///cmd/make_it/456".to_string()),
            }
        );
    }

    #[test]
    fn retrieve_invoking_command_of_sub_sub_command() {
        let topic =
            Topic::new_unchecked("te/device/main///cmd/child/sub:parent:sub:grand-parent:456");
        let payload = r#"{ "status":"failed", "reason":"no idea" }"#;
        let command = mqtt_channel::MqttMessage::new(&topic, payload);
        let cmd = GenericCommandState::from_command_message(&command).expect("parsing error");
        assert!(cmd.is_finished());
        assert!(cmd.is_failed());
        assert_eq!(cmd.failure_reason(), Some("no idea"));
        assert!(!cmd.is_successful());
        assert_eq!(cmd.operation(), Some("child".to_string()));
        assert_eq!(
            cmd.invoking_operation_names(),
            vec!["grand-parent".to_string(), "parent".to_string()]
        );
        assert_eq!(
            cmd,
            GenericCommandState {
                topic: topic.clone(),
                status: "failed".to_string(),
                payload: json!({
                    "status": "failed",
                    "reason": "no idea"
                }),
                invoking_command_topic: Some(
                    "te/device/main///cmd/parent/sub:grand-parent:456".to_string()
                ),
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
