use crate::flow;
use crate::flow::FlowError;
use crate::flow::Message;
use crate::js_runtime::JsRuntime;
use crate::js_value::JsonValue;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::time::SystemTime;
use tracing::debug;

#[derive(Clone, Debug)]
pub struct JsScript {
    pub module_name: String,
    pub flow: Utf8PathBuf,
    pub path: Utf8PathBuf,
    pub is_defined: bool,
    pub is_periodic: bool,
}

impl JsScript {
    pub fn new(module_name: String, flow: Utf8PathBuf, path: Utf8PathBuf) -> Self {
        JsScript {
            module_name,
            flow,
            path,
            is_defined: false,
            is_periodic: false,
        }
    }

    pub fn context(&self, config: &JsonValue) -> JsonValue {
        JsonValue::Context {
            flow: self.flow.to_string(),
            step: self.module_name.to_owned(),
            config: Box::new(config.clone()),
        }
    }

    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    /// Transform an input message into zero, one or more output messages
    ///
    /// The "onMessage" function of the JS module is passed 3 arguments
    /// - the message to be transformed
    /// - the flow step config (as configured in the flow toml)
    ///
    /// The returned value is expected to be an array of messages.
    pub async fn on_message(
        &self,
        js: &JsRuntime,
        timestamp: SystemTime,
        message: &Message,
        config: &JsonValue,
    ) -> Result<Vec<Message>, FlowError> {
        debug!(target: "flows", "{}: onMessage({timestamp:?}, {message})", &self.module_name);
        if !self.is_defined {
            return Ok(vec![message.clone()]);
        }

        let mut message = message.clone();
        if message.timestamp.is_none() {
            message.timestamp = Some(timestamp);
        }
        let input = vec![message.into(), self.context(config)];
        js.call_function(&self.module_name, "onMessage", input)
            .await
            .map_err(flow::error_from_js)?
            .try_into()
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
        timestamp: SystemTime,
        config: &JsonValue,
    ) -> Result<Vec<Message>, FlowError> {
        if !self.is_periodic {
            return Ok(vec![]);
        };
        debug!(target: "flows", "{}: onInterval({timestamp:?})", self.module_name);
        let input = vec![timestamp.into(), self.context(config)];
        js.call_function(&self.module_name, "onInterval", input)
            .await
            .map_err(flow::error_from_js)?
            .try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::js_lib::kv_store::FlowContext;
    use crate::js_lib::kv_store::FlowContextHandle;
    use crate::steps::FlowStep;
    use serde_json::json;
    use std::time::Duration;
    use tedge_mqtt_ext::MqttMessage;

    #[tokio::test]
    async fn identity_script() {
        let js = "export function onMessage(msg) { return [msg]; };";
        let (runtime, mut script) = runtime_with(js).await;

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
        let (runtime, mut script) = runtime_with(js).await;

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
        let (runtime, mut script) = runtime_with(js).await;

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
        let (runtime, mut script) = runtime_with(js).await;

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
        let (runtime, mut script) = runtime_with(js).await;

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
export function onMessage(message, context) {
    const { topic = "topic/not/set" } = context.config
    const td = new globalThis.TextDecoder()
    let groups = message.topic.split( '/')
    let data = td.decode(message.payload).split(':')

    let group = groups[2]
	let measurement = groups[3]
	let time = data[0]
	let value = data[1]

    return [ {
        topic: topic,
        payload: `{"time": ${time}, "${group}": {"${measurement}": ${value}}}`
    }]
}
        "#;
        let (runtime, script) = runtime_with(js).await;
        let mut script = script
            .with_config(Some(json!({
                "topic": "te/device/main///m/collectd"
            })))
            .unwrap();

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
export async function onMessage(message) {
    return [{topic:"foo/bar",payload:`{foo:"bar"}`}];
}
        "#;
        let (runtime, mut script) = runtime_with(js).await;

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
export function onMessage(message) {
    let time = message.time;
    return {
        "topic": message.topic,
        "payload": JSON.stringify({
            "milliseconds": time.getTime(),
            "date": time.toUTCString(),
        })
    }
}
        "#;
        let (runtime, mut script) = runtime_with(js).await;

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
        let (runtime, mut script) = runtime_with(js).await;

        let input = Message::new("dummy", "content");
        let err = script.on_message(&runtime, SystemTime::now(), &input).await;
        assert!(format!("{:?}", err).contains("setTimeout is not defined"));
    }

    #[tokio::test]
    async fn while_loop() {
        let js = r#"export function onMessage(msg) { while(true); };"#;
        let (runtime, mut script) = runtime_with(js).await;

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
        let (runtime, mut script) = runtime_with(js).await;

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
        let (runtime, mut script) = runtime_with(js).await;

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
export async function onMessage(message) {
    const utf8decoder = new TextDecoder();
    const encodedText = message.payload;
    console.log(encodedText);
    const decodedText = utf8decoder.decode(encodedText);
    console.log(decodedText);
    return [{topic:"decoded", payload: decodedText}];
}
        "#;
        let (runtime, mut script) = runtime_with(js).await;

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
export async function onMessage(message) {
    const utf8encoder = new TextEncoder();
    const utf8decoder = new TextDecoder();
    const payload = utf8decoder.decode(message.payload);
    console.log(payload);
    const encodedText = utf8encoder.encode(payload);
    console.log(encodedText);
    return [{topic:"encoded", payload: encodedText}];
}
        "#;
        let (runtime, mut script) = runtime_with(js).await;

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
const utf8 = new TextDecoder();

export async function onMessage(message) {
    const encodedText = message.payload;
    const decodedText = utf8.decode(encodedText);
    return [{topic:"decoded", payload: decodedText}];
}
        "#;
        let (runtime, mut script) = runtime_with(js).await;

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
export async function onMessage(message) {
    const utf8encoder = new TextEncoder();
    const utf8decoder = new TextDecoder();
    const payload = utf8decoder.decode(message.payload);
    const u8array = new Uint8Array(8);
    const result = utf8encoder.encodeInto(payload, u8array);
    console.log(result);
    utf8encoder.encodeInto(payload, u8array.subarray(4));
    return [{topic:"encoded", payload: u8array}];
}
        "#;
        let (runtime, mut script) = runtime_with(js).await;

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
export async function onMessage(message) {
    const te = new globalThis.TextEncoder();
    const td = new globalThis.TextDecoder();

    const encodedText = message.payload;
    const decodedText = td.decode(encodedText);
    const finalPayload = te.encode(decodedText + decodedText);
    return [{topic:"decoded", payload: finalPayload}];
}
        "#;
        let (runtime, mut script) = runtime_with(js).await;

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
export async function onMessage(message) {
    const measurements = new Uint32Array(message.payload.buffer);

    const tedge_json = {
        time: measurements[0],
        value: measurements[1]
    }

    return [{topic:"decoded", payload: JSON.stringify(tedge_json)}];
}
        "#;
        let (runtime, mut script) = runtime_with(js).await;

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

    #[tokio::test]
    async fn using_the_context() {
        let js = r#"
export function onMessage(message, context) {
    let payload = context.mapper.get(message.topic);
    let fragment = context.script.get(message.topic);
    Object.assign(payload, fragment)
    return {
        topic: message.topic,
        payload: JSON.stringify(payload)
    }
}
        "#;
        let (runtime, mut script) = runtime_with(js).await;

        runtime.context_handle().insert(
            &FlowContext::Mapper,
            "foo/bar",
            serde_json::json!({
                "guess": 42,
            }),
        );

        runtime.context_handle().insert(
            &FlowContext::script(script.step_name()),
            "foo/bar",
            serde_json::json!({
                "hello": "world",
            }),
        );

        let input = Message::new("foo/bar", "");
        let output = Message::new("foo/bar", r#"{"guess":42,"hello":"world"}"#);
        assert_eq!(
            script
                .on_message(&runtime, SystemTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn listing_context_keys() {
        let js = r#"
export function onMessage(message, context) {
    let entities = {}
    for (const key of context.mapper.keys()) {
        const entity = context.mapper.get(key)
        entities[key] = entity["external_id"]
    }
    return {
        topic: message.topic,
        payload: JSON.stringify(entities)
    }
}
        "#;
        let (runtime, mut script) = runtime_with(js).await;

        runtime.context_handle().insert(
            &FlowContext::Mapper,
            "device/main///",
            serde_json::json!({
                "external_id": "Raspberry-123",
            }),
        );
        runtime.context_handle().insert(
            &FlowContext::Mapper,
            "device/child-01///",
            serde_json::json!({
                "external_id": "Raspberry-123:child-01",
            }),
        );

        let input = Message::new("foo/bar", "");
        let output = Message::new(
            "foo/bar",
            r#"{"device/child-01///":"Raspberry-123:child-01","device/main///":"Raspberry-123"}"#,
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
    async fn updating_the_context() {
        let js = r#"
export function onMessage(message, context) {
    let count = context.script.get("count") || 0;
    context.script.set("count", count + 1);
    return message
}
        "#;
        let (runtime, mut script) = runtime_with(js).await;

        let input = Message::new("foo/bar", "");
        let context = FlowContext::script(script.step_name());

        script
            .on_message(&runtime, SystemTime::now(), &input)
            .await
            .unwrap();
        assert_eq!(
            runtime.context_handle().get(&context, "count"),
            JsonValue::Number(1.into())
        );

        script
            .on_message(&runtime, SystemTime::now(), &input)
            .await
            .unwrap();
        assert_eq!(
            runtime.context_handle().get(&context, "count"),
            JsonValue::Number(2.into())
        );
    }

    #[tokio::test]
    async fn removing_keys_from_the_context() {
        let js = r#"
export function onMessage(message, context) {
    context.mapper.set("foo", null)
    context.mapper.remove("bar")
    return message
}
        "#;

        let (runtime, mut script) = runtime_with(js).await;
        runtime.context_handle().insert(
            &FlowContext::Mapper,
            "foo",
            serde_json::json!({
                "a": 1,
            }),
        );
        runtime.context_handle().insert(
            &FlowContext::Mapper,
            "bar",
            serde_json::json!({
                "b": 2,
            }),
        );

        let input = Message::new("foo/bar", "");

        script
            .on_message(&runtime, SystemTime::now(), &input)
            .await
            .unwrap();
        assert_eq!(
            runtime.context_handle().get(&FlowContext::Mapper, "foo"),
            JsonValue::Null
        );
        assert_eq!(
            runtime.context_handle().get(&FlowContext::Mapper, "bar"),
            JsonValue::Null
        );
    }

    #[tokio::test]
    async fn setting_protocol_specific_properties() {
        let js = r#"
export function onMessage(message) {
    message.mqtt = {
        "qos": 2,
        "retain": true,
    };
    return message
}
        "#;
        let (runtime, mut script) = runtime_with(js).await;

        let input = Message::new("foo/bar", "some message");
        let output = script
            .on_message(&runtime, SystemTime::now(), &input)
            .await
            .unwrap()
            .pop()
            .unwrap();

        let mqtt_message = MqttMessage::try_from(output).unwrap();
        assert_eq!(mqtt_message.qos, tedge_mqtt_ext::QoS::ExactlyOnce,);
        assert!(mqtt_message.retain,);
    }

    async fn runtime_with(js: &str) -> (JsRuntime, FlowStep) {
        let context = FlowContextHandle::default();
        let mut runtime = JsRuntime::try_new(context).await.unwrap();
        let mut script = JsScript::new("toml|1|js".to_owned(), "toml".into(), "js".into());
        if let Err(err) = runtime.load_script_literal(&mut script, js).await {
            panic!("{:?}", err);
        }
        let step = FlowStep::new_script(script);
        (runtime, step)
    }
}
