use crate::flow;
use crate::flow::DateTime;
use crate::flow::FlowError;
use crate::flow::Message;
use crate::js_runtime::JsRuntime;
use crate::js_value::JsonValue;
use std::path::Path;
use std::path::PathBuf;
use tracing::debug;

#[derive(Clone)]
pub struct JsScript {
    pub module_name: String,
    pub path: PathBuf,
    pub config: JsonValue,
    pub interval_secs: u64,
    pub no_js_on_message_fun: bool,
    pub no_js_on_config_update_fun: bool,
    pub no_js_on_interval_fun: bool,
}

impl JsScript {
    pub fn new(flow: PathBuf, index: usize, path: PathBuf) -> Self {
        let module_name = format!("{}|{}|{}", flow.display(), index, path.display());
        JsScript {
            module_name,
            path,
            config: JsonValue::default(),
            interval_secs: 0,
            no_js_on_message_fun: true,
            no_js_on_config_update_fun: true,
            no_js_on_interval_fun: true,
        }
    }

    pub fn module_name(&self) -> String {
        self.module_name.to_owned()
    }

    pub fn with_config(self, config: Option<serde_json::Value>) -> Self {
        if let Some(config) = config {
            Self {
                config: JsonValue::from(config),
                ..self
            }
        } else {
            self
        }
    }

    pub fn with_interval_secs(self, interval_secs: u64) -> Self {
        Self {
            interval_secs,
            ..self
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn source(&self) -> String {
        format!("{}", self.path.display())
    }

    /// Transform an input message into zero, one or more output messages
    ///
    /// The "onMessage" function of the JS module is passed 3 arguments
    /// - the current timestamp
    /// - the message to be transformed
    /// - the flow step config (as configured for the flow step, possibly updated by onConfigUpdate messages)
    ///
    /// The returned value is expected to be an array of messages.
    pub async fn on_message(
        &self,
        js: &JsRuntime,
        timestamp: &DateTime,
        message: &Message,
    ) -> Result<Vec<Message>, FlowError> {
        debug!(target: "flows", "{}: onMessage({timestamp:?}, {message})", self.module_name());
        if self.no_js_on_message_fun {
            return Ok(vec![message.clone()]);
        }

        let mut message = message.clone();
        if message.timestamp.is_none() {
            message.timestamp = Some(timestamp.clone());
        }
        let input = vec![message.into(), self.config.clone()];
        js.call_function(&self.module_name(), "onMessage", input)
            .await
            .map_err(flow::error_from_js)?
            .try_into()
    }

    /// Update the flow step config using a metadata message
    ///
    /// The "onConfigUpdate" function of the JS module is passed 2 arguments
    /// - the message
    /// - the current flow step config
    ///
    /// The value returned by this function is used as the updated flow step config
    pub async fn on_config_update(
        &mut self,
        js: &JsRuntime,
        message: &Message,
    ) -> Result<(), FlowError> {
        debug!(target: "flows", "{}: onConfigUpdate({message})", self.module_name());
        if self.no_js_on_config_update_fun {
            return Ok(());
        }

        let input = vec![message.clone().into(), self.config.clone()];
        let config = js
            .call_function(&self.module_name(), "onConfigUpdate", input)
            .await
            .map_err(flow::error_from_js)?;
        self.config = config;
        Ok(())
    }

    /// Trigger the onInterval function of the JS module
    ///
    /// The "onInterval" function is passed 2 arguments
    /// - the current timestamp
    /// - the current flow step config
    ///
    /// Return zero, one or more messages
    pub async fn on_interval(
        &self,
        js: &JsRuntime,
        timestamp: &DateTime,
    ) -> Result<Vec<Message>, FlowError> {
        if self.no_js_on_interval_fun {
            return Ok(vec![]);
        }
        if !timestamp.tick_now(self.interval_secs) {
            return Ok(vec![]);
        }
        debug!(target: "flows", "{}: onInterval({timestamp:?})", self.module_name());
        let input = vec![timestamp.clone().into(), self.config.clone()];
        js.call_function(&self.module_name(), "onInterval", input)
            .await
            .map_err(flow::error_from_js)?
            .try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn identity_script() {
        let js = "export function onMessage(msg) { return [msg]; };";
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("te/main/device///m/", "hello world");
        let output = input.clone();
        assert_eq!(
            script
                .on_message(&runtime, &DateTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn identity_script_no_array() {
        let js = "export function onMessage(msg) { return msg; };";
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("te/main/device///m/", "hello world");
        let output = input.clone();
        assert_eq!(
            script
                .on_message(&runtime, &DateTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn script_returning_null() {
        let js = "export function onMessage(msg) { return null; };";
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("te/main/device///m/", "hello world");
        assert_eq!(
            script
                .on_message(&runtime, &DateTime::now(), &input)
                .await
                .unwrap(),
            vec![]
        );
    }

    #[tokio::test]
    async fn script_returning_nothing() {
        let js = "export function onMessage(msg) { return; };";
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("te/main/device///m/", "hello world");
        assert_eq!(
            script
                .on_message(&runtime, &DateTime::now(), &input)
                .await
                .unwrap(),
            vec![]
        );
    }

    #[tokio::test]
    async fn error_script() {
        let js = r#"export function onMessage(msg) { throw new Error("Cannot process that message"); };"#;
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("te/main/device///m/", "hello world");
        let error = script
            .on_message(&runtime, &DateTime::now(), &input)
            .await
            .unwrap_err();
        eprintln!("{:?}", error);
        assert!(error.to_string().contains("Cannot process that message"));
    }

    #[tokio::test]
    async fn collectd_script() {
        let js = r#"
export function onMessage(message, config) {
    let groups = message.topic.split( '/')
    let data = message.payload.split(':')

    let group = groups[2]
	let measurement = groups[3]
	let time = data[0]
	let value = data[1]

    var topic = "te/device/main///m/collectd"
    if (config && config.topic) {
        topic = config.topic
    }

    return [ {
        topic: topic,
        payload: `{"time": ${time}, "${group}": {"${measurement}": ${value}}}`
    }]
}
        "#;
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new(
            "collectd/h/memory/percent-used",
            "1748440192.104:19.9289468288182",
        );
        let mut output = Message::new(
            "te/device/main///m/collectd",
            r#"{"time": 1748440192.104, "memory": {"percent-used": 19.9289468288182}}"#,
        );
        output.timestamp = None;
        assert_eq!(
            script
                .on_message(&runtime, &DateTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn promise_script() {
        let js = r#"
export async function onMessage(message, config) {
    return [{topic:"foo/bar",payload:`{foo:"bar"}`}];
}
        "#;
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("dummy", "content");
        let mut output = Message::new("foo/bar", r#"{foo:"bar"}"#);
        output.timestamp = None;
        assert_eq!(
            script
                .on_message(&runtime, &DateTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn using_unknown_function() {
        let js = r#"
function transform(x) { return [x] }
export function onMessage(message) {
    return setTimeout(transform, 1000, message);
}
        "#;
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("dummy", "content");
        let err = script.on_message(&runtime, &DateTime::now(), &input).await;
        assert!(format!("{:?}", err).contains("setTimeout is not defined"));
    }

    #[tokio::test]
    async fn while_loop() {
        let js = r#"export function onMessage(msg) { while(true); };"#;
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("topic", "payload");
        let error = script
            .on_message(&runtime, &DateTime::now(), &input)
            .await
            .unwrap_err();
        eprintln!("{:?}", error);
        assert!(error.to_string().contains("interrupted"));
    }

    #[tokio::test]
    async fn memory_eager_loop() {
        let js = r#"export function onMessage(msg) { var s = "foo"; while(true) { s += s; }; };"#;
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("topic", "payload");
        let error = script
            .on_message(&runtime, &DateTime::now(), &input)
            .await
            .unwrap_err();
        eprintln!("{:?}", error);
        assert!(error.to_string().contains("out of memory"));
    }

    #[tokio::test]
    async fn stack_eager_loop() {
        let js = r#"export function onMessage(msg) { return onMessage(msg); };"#;
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("topic", "payload");
        let error = script
            .on_message(&runtime, &DateTime::now(), &input)
            .await
            .unwrap_err();
        eprintln!("{:?}", error);
        assert!(error
            .to_string()
            .contains("Maximum call stack size exceeded"));
    }

    #[tokio::test]
    async fn using_text_decoder() {
        let js = r#"
export async function onMessage(message, config) {
    const utf8decoder = new TextDecoder();
    const encodedText = message.raw_payload;
    console.log(encodedText);
    const decodedText = utf8decoder.decode(encodedText);
    console.log(decodedText);
    return [{topic:"decoded", payload: decodedText}];
}
        "#;
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new_binary("encoded", [240, 159, 146, 150]);
        let mut output = Message::new("decoded", "💖");
        output.timestamp = None;
        assert_eq!(
            script
                .on_message(&runtime, &DateTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn using_text_encoder() {
        let js = r#"
export async function onMessage(message, config) {
    const utf8encoder = new TextEncoder();
    console.log(message.payload);
    const encodedText = utf8encoder.encode(message.payload);
    console.log(encodedText);
    return [{topic:"encoded", payload: encodedText}];
}
        "#;
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("decoded", "💖");
        let mut output = Message::new_binary("encoded", [240, 159, 146, 150]);
        output.timestamp = None;
        assert_eq!(
            script
                .on_message(&runtime, &DateTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }

    async fn runtime_with(js: &str) -> (JsRuntime, JsScript) {
        let mut runtime = JsRuntime::try_new().await.unwrap();
        let mut script = JsScript::new("toml".into(), 1, "js".into());
        if let Err(err) = runtime.load_js(script.module_name(), js).await {
            panic!("{:?}", err);
        }
        script.no_js_on_message_fun = false;
        (runtime, script)
    }
}
