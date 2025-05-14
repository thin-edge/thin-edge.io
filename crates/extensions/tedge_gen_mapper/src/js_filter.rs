use crate::pipeline;
use crate::pipeline::FilterError;
use crate::LoadError;
use rustyscript::deno_core::ModuleId;
use rustyscript::serde_json::json;
use rustyscript::worker::DefaultWorker;
use rustyscript::worker::DefaultWorkerOptions;
use rustyscript::Error;
use rustyscript::Module;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tedge_mqtt_ext::MqttMessage;
use time::OffsetDateTime;
use tracing::debug;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct DateTime {
    seconds: u64,
    nanoseconds: u32,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Message {
    topic: String,
    payload: String,
}

#[derive(Clone)]
pub struct JsFilter {
    path: PathBuf,
    module_id: ModuleId,
}

impl JsFilter {
    pub fn process(
        &self,
        js: &JsRuntime,
        timestamp: OffsetDateTime,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, FilterError> {
        debug!(target: "MAPPING", "{}: process({timestamp}, {message:?})", self.path.display());
        let timestamp = DateTime::try_from(timestamp)?;
        let message = Message::try_from(message)?;
        let input = vec![
            json!({"seconds": timestamp.seconds, "nanoseconds": timestamp.nanoseconds}),
            json!({"topic": message.topic, "payload": message.payload}),
        ];
        let output: Vec<Message> = js
            .runtime
            .call_function(Some(self.module_id), "process".to_string(), input)
            .map_err(error_from_js)?;
        output.into_iter().map(MqttMessage::try_from).collect()
    }

    pub fn update_config(&self, _js: &JsRuntime, config: &MqttMessage) -> Result<(), FilterError> {
        debug!(target: "MAPPING", "{}: update_config({config:?})", self.path.display());
        Ok(())
    }

    pub fn tick(
        &self,
        _js: &JsRuntime,
        timestamp: OffsetDateTime,
    ) -> Result<Vec<MqttMessage>, FilterError> {
        debug!(target: "MAPPING", "{}: tick({timestamp})", self.path.display());
        Ok(vec![])
    }
}

pub struct JsRuntime {
    runtime: DefaultWorker,
    modules: HashMap<PathBuf, ModuleId>,
}

impl JsRuntime {
    pub fn try_new() -> Result<Self, LoadError> {
        let runtime = DefaultWorker::new(DefaultWorkerOptions {
            default_entrypoint: None,
            timeout: Duration::from_millis(100),
            ..Default::default()
        })?;
        let modules = HashMap::new();
        Ok(JsRuntime { runtime, modules })
    }

    #[cfg(test)]
    pub fn load_js(
        &mut self,
        path: impl AsRef<Path>,
        script: impl ToString,
    ) -> Result<JsFilter, LoadError> {
        let module = Module::new(path, script);
        self.load(module)
    }

    pub fn load_file(&mut self, path: impl AsRef<Path>) -> Result<JsFilter, LoadError> {
        let module = Module::load(path)?;
        self.load(module)
    }

    pub fn load(&mut self, module: Module) -> Result<JsFilter, LoadError> {
        let path = module.filename().to_owned();
        let module_id = self.runtime.load_module(module)?;
        self.modules.insert(path.clone(), module_id);
        Ok(JsFilter { path, module_id })
    }

    pub fn loaded_module(&self, path: PathBuf) -> Result<JsFilter, LoadError> {
        match self.modules.get(&path).cloned() {
            None => Err(LoadError::ScriptNotLoaded { path }),
            Some(module_id) => Ok(JsFilter { path, module_id }),
        }
    }
}

impl TryFrom<OffsetDateTime> for DateTime {
    type Error = FilterError;

    fn try_from(value: OffsetDateTime) -> Result<Self, Self::Error> {
        let seconds = u64::try_from(value.unix_timestamp()).map_err(|err| {
            FilterError::UnsupportedMessage(format!("failed to convert timestamp: {}", err))
        })?;

        Ok(DateTime {
            seconds,
            nanoseconds: value.nanosecond(),
        })
    }
}

impl TryFrom<&MqttMessage> for Message {
    type Error = FilterError;

    fn try_from(message: &MqttMessage) -> Result<Self, Self::Error> {
        let topic = message.topic.to_string();
        let payload = message
            .payload_str()
            .map_err(|_| {
                pipeline::FilterError::UnsupportedMessage("Not an UTF8 payload".to_string())
            })?
            .to_string();
        Ok(Message { topic, payload })
    }
}

impl TryFrom<Message> for MqttMessage {
    type Error = FilterError;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        let topic = message.topic.as_str().try_into().map_err(|_| {
            FilterError::UnsupportedMessage(format!("invalid topic {}", message.topic))
        })?;
        Ok(MqttMessage::new(&topic, message.payload))
    }
}

fn error_from_js(err: Error) -> FilterError {
    match err {
        Error::Runtime(err) => FilterError::UnsupportedMessage(err),
        Error::JsError(err) => FilterError::UnsupportedMessage(err.exception_message),
        err => FilterError::IncorrectSetting(format!("{}", err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_mqtt_ext::Topic;

    #[test]
    fn identity_filter() {
        let script = "export function process(t,msg) { return [msg]; };";
        let mut runtime = JsRuntime::try_new().unwrap();
        let filter = runtime.load_js("id.js", script).unwrap();

        let topic = Topic::new_unchecked("te/main/device///m/");
        let input = MqttMessage::new(&topic, "hello world");
        let output = input.clone();
        assert_eq!(
            filter
                .process(&runtime, OffsetDateTime::now_utc(), &input)
                .unwrap(),
            vec![output]
        );
    }

    #[test]
    fn error_filter() {
        let script = r#"export function process(t,msg) { throw new Error("Cannot process that message"); };"#;
        let mut runtime = JsRuntime::try_new().unwrap();
        let filter = runtime.load_js("err.js", script).unwrap();

        let topic = Topic::new_unchecked("te/main/device///m/");
        let input = MqttMessage::new(&topic, "hello world");
        let error = filter
            .process(&runtime, OffsetDateTime::now_utc(), &input)
            .unwrap_err();
        assert!(error.to_string().contains("Cannot process that message"));
    }
}
