use crate::flow;
use crate::flow::FlowError;
use crate::flow::Message;
use crate::js_runtime::JsRuntime;
use crate::js_value::JsonValue;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::time::Duration;
use std::time::SystemTime;
use tokio::time::Instant;
use tracing::debug;

#[derive(Clone, Debug)]
pub struct JsScript {
    pub module_name: String,
    pub path: Utf8PathBuf,
    pub config: JsonValue,
    pub interval: Duration,
    pub next_execution: Option<Instant>,
    pub no_js_on_message_fun: bool,
    pub no_js_on_config_update_fun: bool,
    pub no_js_on_interval_fun: bool,
}

impl JsScript {
    pub fn new(flow: Utf8PathBuf, index: usize, path: Utf8PathBuf) -> Self {
        let module_name = format!("{flow}|{index}|{path}");
        JsScript {
            module_name,
            path,
            config: JsonValue::default(),
            interval: Duration::ZERO,
            next_execution: None,
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

    pub fn with_interval(self, interval: Duration) -> Self {
        Self { interval, ..self }
    }

    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    pub fn source(&self) -> String {
        self.path.to_string()
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
        timestamp: SystemTime,
        message: &Message,
    ) -> Result<Vec<Message>, FlowError> {
        debug!(target: "flows", "{}: onMessage({timestamp:?}, {message})", self.module_name());
        if self.no_js_on_message_fun {
            return Ok(vec![message.clone()]);
        }

        let mut message = message.clone();
        if message.timestamp.is_none() {
            message.timestamp = Some(timestamp);
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

    /// Initialize the next execution time for this script's interval
    /// Should be called after the script is loaded and interval is set
    pub fn init_next_execution(&mut self) {
        if !self.no_js_on_interval_fun && !self.interval.is_zero() {
            self.next_execution = Some(Instant::now() + self.interval);
        }
    }

    /// Check if this script should execute its interval function now
    /// Returns true and updates next_execution if it's time to execute
    pub fn should_execute_interval(&mut self, now: Instant) -> bool {
        if self.no_js_on_interval_fun || self.interval.is_zero() {
            return false;
        }

        match self.next_execution {
            Some(deadline) if now >= deadline => {
                // Time to execute - schedule next execution
                self.next_execution = Some(now + self.interval);
                true
            }
            None => {
                // First execution - initialize and execute
                self.next_execution = Some(now + self.interval);
                true
            }
            _ => false,
        }
    }

    /// Trigger the onInterval function of the JS module
    ///
    /// The "onInterval" function is passed 2 arguments
    /// - the current timestamp
    /// - the current flow step config
    ///
    /// Return zero, one or more messages
    ///
    /// Note: Caller should check should_execute_interval() before calling this
    pub async fn on_interval(
        &self,
        js: &JsRuntime,
        timestamp: SystemTime,
    ) -> Result<Vec<Message>, FlowError> {
        debug!(target: "flows", "{}: onInterval({timestamp:?})", self.module_name());
        let input = vec![timestamp.into(), self.config.clone()];
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
                .on_message(&runtime, SystemTime::now(), &input)
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
                .on_message(&runtime, SystemTime::now(), &input)
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
                .on_message(&runtime, SystemTime::now(), &input)
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
                .on_message(&runtime, SystemTime::now(), &input)
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
            .on_message(&runtime, SystemTime::now(), &input)
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
        let output = Message::new(
            "te/device/main///m/collectd",
            r#"{"time": 1748440192.104, "memory": {"percent-used": 19.9289468288182}}"#,
        );
        assert_eq!(
            script
                .on_message(&runtime, SystemTime::now(), &input)
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
        let output = Message::new("foo/bar", r#"{foo:"bar"}"#);
        assert_eq!(
            script
                .on_message(&runtime, SystemTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn using_date() {
        let js = r#"
export function onMessage(message, config) {
    let time = message.timestamp;
    return {
        "topic": message.topic,
        "payload": JSON.stringify({
            "milliseconds": time.getTime(),
            "date": time.toUTCString(),
        })
    }
}
        "#;
        let (runtime, script) = runtime_with(js).await;

        let datetime = SystemTime::UNIX_EPOCH + Duration::from_secs(1763050414);
        let input = Message::new("clock", "");
        let output = Message::new(
            "clock",
            r#"{"milliseconds":1763050414000,"date":"Thu, 13 Nov 2025 16:13:34 GMT"}"#.to_string(),
        );
        assert_eq!(
            script.on_message(&runtime, datetime, &input).await.unwrap(),
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
        let err = script.on_message(&runtime, SystemTime::now(), &input).await;
        assert!(format!("{:?}", err).contains("setTimeout is not defined"));
    }

    #[tokio::test]
    async fn while_loop() {
        let js = r#"export function onMessage(msg) { while(true); };"#;
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("topic", "payload");
        let error = script
            .on_message(&runtime, SystemTime::now(), &input)
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
            .on_message(&runtime, SystemTime::now(), &input)
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
            .on_message(&runtime, SystemTime::now(), &input)
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

        let input = Message::new("encoded", [240, 159, 146, 150]);
        let output = Message::new("decoded", "💖");
        assert_eq!(
            script
                .on_message(&runtime, SystemTime::now(), &input)
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
        let output = Message::new("encoded", [240, 159, 146, 150]);
        assert_eq!(
            script
                .on_message(&runtime, SystemTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn decode_utf8_with_bom_and_invalid_chars() {
        let js = r#"
export async function onMessage(message, config) {
    const utf8decoder = new TextDecoder();
    const encodedText = message.raw_payload;
    const decodedText = utf8decoder.decode(encodedText);
    return [{topic:"decoded", payload: decodedText}];
}
        "#;
        let (runtime, script) = runtime_with(js).await;

        let utf8_with_bom_and_invalid_chars = b"\xEF\xBB\xBFHello \xF0\x90\x80World";
        let input = Message::new("encoded", utf8_with_bom_and_invalid_chars);
        let output = Message::new("decoded", "Hello �World");
        assert_eq!(
            script
                .on_message(&runtime, SystemTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn using_text_encoder_into() {
        let js = r#"
export async function onMessage(message, config) {
    const utf8encoder = new TextEncoder();
    const u8array = new Uint8Array(8);
    const result = utf8encoder.encodeInto(message.payload, u8array);
    console.log(result);
    utf8encoder.encodeInto(message.payload, u8array.subarray(4));
    return [{topic:"encoded", payload: u8array}];
}
        "#;
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("decoded", "💖");
        let output = Message::new("encoded", [240, 159, 146, 150, 240, 159, 146, 150]);
        assert_eq!(
            script
                .on_message(&runtime, SystemTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn using_standard_built_in_objects() {
        let js = r#"
export async function onMessage(message, config) {
    const te = new globalThis.TextEncoder();
    const td = new globalThis.TextDecoder();

    const encodedText = message.raw_payload;
    const decodedText = td.decode(encodedText);
    const finalPayload = te.encode(decodedText + decodedText);
    return [{topic:"decoded", payload: finalPayload}];
}
        "#;
        let (runtime, script) = runtime_with(js).await;

        let input = Message::new("encoded", [240, 159, 146, 150]);
        let output = Message::new("decoded", [240, 159, 146, 150, 240, 159, 146, 150]);
        assert_eq!(
            script
                .on_message(&runtime, SystemTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn reading_raw_integers() {
        let js = r#"
export async function onMessage(message, config) {
    const measurements = new Uint32Array(message.raw_payload.buffer);

    const tedge_json = {
        time: measurements[0],
        value: measurements[1]
    }

    return [{topic:"decoded", payload: JSON.stringify(tedge_json)}];
}
        "#;
        let (runtime, script) = runtime_with(js).await;

        let time = 1758212648u32.to_le_bytes();
        let value = 12345u32.to_le_bytes();
        let input = Message::new("encoded", [time, value].as_flattened());
        let output = Message::new("decoded", r#"{"time":1758212648,"value":12345}"#);
        assert_eq!(
            script
                .on_message(&runtime, SystemTime::now(), &input)
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
