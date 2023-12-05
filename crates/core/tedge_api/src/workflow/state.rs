use crate::workflow::WorkflowExecutionError;
use mqtt_channel::Message;
use mqtt_channel::QoS::AtLeastOnce;
use mqtt_channel::Topic;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;

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

impl GenericCommandState {
    /// Extract a command state from a json payload
    pub fn from_command_message(message: &Message) -> Result<Option<Self>, WorkflowExecutionError> {
        let payload = message.payload_bytes();
        if payload.is_empty() {
            return Ok(None);
        }
        let topic = message.topic.clone();
        let json: Value = serde_json::from_slice(payload)?;
        let status = GenericCommandState::extract_text_property(&json, "status")
            .ok_or(WorkflowExecutionError::MissingStatus)?;
        Ok(Some(GenericCommandState {
            topic,
            status,
            payload: json,
        }))
    }

    pub fn into_message(self) -> Message {
        let topic = &self.topic;
        let payload = self.payload.to_string();
        Message::new(topic, payload)
            .with_retain()
            .with_qos(AtLeastOnce)
    }

    /// Serialize the command state as a json payload
    pub fn to_json_string(mut self) -> String {
        GenericCommandState::inject_text_property(&mut self.payload, "status", &self.status);
        self.payload.to_string()
    }

    /// Inject a json payload into this one
    pub fn update_from_json(mut self, json: Value) -> Self {
        if let (Some(values), Some(new_values)) = (self.payload.as_object_mut(), json.as_object()) {
            for (k, v) in new_values {
                values.insert(k.to_string(), v.clone());
            }
        }
        match GenericCommandState::extract_text_property(&self.payload, "status") {
            None => self.fail_with("Unknown status".to_string()),
            Some(status) => GenericCommandState { status, ..self },
        }
    }

    /// Update the command state with the outcome of a script
    pub fn update_with_script_output(
        self,
        script: String,
        output: std::io::Result<std::process::Output>,
    ) -> Self {
        match output {
            Ok(output) => {
                if output.status.success() {
                    match String::from_utf8(output.stdout)
                        .ok()
                        .and_then(extract_script_output)
                    {
                        Some(stdout) => match serde_json::from_str(&stdout) {
                            Ok(json) => self.update_from_json(json),
                            Err(err) => {
                                let reason =
                                    format!("Script {script} returned non JSON stdout: {err}");
                                self.fail_with(reason)
                            }
                        },
                        None => {
                            let reason = format!("Script {script} returned no tedge output");
                            self.fail_with(reason)
                        }
                    }
                } else {
                    match String::from_utf8(output.stderr) {
                        Ok(stderr) => {
                            let reason = format!("Script {script} failed with: {stderr}");
                            self.fail_with(reason)
                        }
                        Err(_) => {
                            let reason =
                                format!("Script {script} failed and returned non UTF-8 stderr");
                            self.fail_with(reason)
                        }
                    }
                }
            }
            Err(err) => {
                let reason = format!("Failed to launch {script}: {err}");
                self.fail_with(reason)
            }
        }
    }

    /// Update the command state with a new status describing the next state
    pub fn move_to(mut self, status: String) -> Self {
        GenericCommandState::inject_text_property(&mut self.payload, "status", &status);

        GenericCommandState { status, ..self }
    }

    /// Update the command state to failed status with the given reason
    pub fn fail_with(mut self, reason: String) -> Self {
        let status = "failed";
        GenericCommandState::inject_text_property(&mut self.payload, "status", status);
        GenericCommandState::inject_text_property(&mut self.payload, "reason", &reason);

        GenericCommandState {
            status: status.to_owned(),
            ..self
        }
    }

    /// Return the error reason if any
    pub fn failure_reason(&self) -> Option<String> {
        GenericCommandState::extract_text_property(&self.payload, "reason")
    }

    /// Extract a text property from a Json object
    fn extract_text_property(json: &Value, property: &str) -> Option<String> {
        json.as_object()
            .and_then(|o| o.get(property))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
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
    /// - Then each token matching `${x.y.z}` is substituted with the value of
    pub fn inject_parameters(&self, args: &[String]) -> Vec<String> {
        args.iter().map(|arg| self.inject_parameter(arg)).collect()
    }

    /// Inject values extracted from the message payload into a script argument
    ///
    /// `${.payload}` -> the whole message payload
    /// `${.payload.x}` -> the value of x if there is any in the payload
    /// `${.payload.unknown}` -> `${.payload.unknown}` unchanged
    /// `Not a variable pattern` -> `Not a variable pattern` unchanged
    pub fn inject_parameter(&self, script_parameter: &str) -> String {
        script_parameter
            .strip_prefix("${")
            .and_then(|s| s.strip_suffix('}'))
            .and_then(|path| self.extract(path))
            .unwrap_or_else(|| script_parameter.to_string())
    }

    fn extract(&self, path: &str) -> Option<String> {
        match path {
            "." => Some(
                json!({
                    "topic": self.topic.name,
                    "payload": self.payload
                })
                .to_string(),
            ),
            ".topic" => Some(self.topic.name.clone()),
            ".topic.target" => self.target(),
            ".topic.operation" => self.operation(),
            ".topic.cmd_id" => self.cmd_id(),
            ".payload" => Some(self.payload.to_string()),
            path => path
                .strip_prefix(".payload.")
                .and_then(|path| json_excerpt(&self.payload, path)),
        }
    }

    fn target(&self) -> Option<String> {
        match self.topic.name.split('/').collect::<Vec<&str>>()[..] {
            [_, t1, t2, t3, t4, "cmd", _, _] => Some(format!("{t1}/{t2}/{t3}/{t4}")),
            _ => None,
        }
    }

    fn operation(&self) -> Option<String> {
        match self.topic.name.split('/').collect::<Vec<&str>>()[..] {
            [_, _, _, _, _, "cmd", operation, _] => Some(operation.to_string()),
            _ => None,
        }
    }

    fn cmd_id(&self) -> Option<String> {
        match self.topic.name.split('/').collect::<Vec<&str>>()[..] {
            [_, _, _, _, _, "cmd", _, cmd_id] => Some(cmd_id.to_string()),
            _ => None,
        }
    }
}

fn json_excerpt(value: &Value, path: &str) -> Option<String> {
    match path.split_once('.') {
        None if path.is_empty() => Some(json_as_string(value)),
        None => value.get(path).map(json_as_string),
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

fn extract_script_output(stdout: String) -> Option<String> {
    if let Some((_, script_output_and_more)) = stdout.split_once(":::begin-tedge:::\n") {
        if let Some((script_output, _)) = script_output_and_more.split_once("\n:::end-tedge:::") {
            return Some(script_output.to_string());
        }
    }
    None
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
        let command = mqtt_channel::Message::new(&topic, payload);
        let cmd = GenericCommandState::from_command_message(&command)
            .expect("parsing error")
            .expect("no message");
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
        let command = mqtt_channel::Message::new(&topic, payload);
        let cmd = GenericCommandState::from_command_message(&command)
            .expect("parsing error")
            .expect("no message");

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
