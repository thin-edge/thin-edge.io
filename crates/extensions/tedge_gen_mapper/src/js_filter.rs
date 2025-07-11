use crate::js_runtime::JsRuntime;
use crate::pipeline;
use crate::pipeline::DateTime;
use crate::pipeline::FilterError;
use crate::pipeline::Message;
use anyhow::Context;
use rquickjs::Ctx;
use rquickjs::FromJs;
use rquickjs::IntoJs;
use rquickjs::Value;
use std::path::Path;
use std::path::PathBuf;
use tracing::debug;

#[derive(Clone)]
pub struct JsFilter {
    pub module_name: String,
    pub path: PathBuf,
    pub config: JsonValue,
    pub tick_every_seconds: u64,
    pub no_js_process: bool,
    pub no_js_update_config: bool,
    pub no_js_tick: bool,
}

#[derive(Clone, Debug)]
pub struct JsonValue(serde_json::Value);

impl Default for JsonValue {
    fn default() -> Self {
        JsonValue(serde_json::Value::Object(Default::default()))
    }
}

impl JsFilter {
    pub fn new(pipeline: PathBuf, index: usize, path: PathBuf) -> Self {
        let module_name = format!("{}|{}|{}", pipeline.display(), index, path.display());
        JsFilter {
            module_name,
            path,
            config: JsonValue::default(),
            tick_every_seconds: 0,
            no_js_process: true,
            no_js_update_config: true,
            no_js_tick: true,
        }
    }

    pub fn module_name(&self) -> String {
        self.module_name.to_owned()
    }

    pub fn with_config(self, config: Option<serde_json::Value>) -> Self {
        if let Some(config) = config {
            Self {
                config: JsonValue(config),
                ..self
            }
        } else {
            self
        }
    }

    pub fn with_tick_every_seconds(self, tick_every_seconds: u64) -> Self {
        Self {
            tick_every_seconds,
            ..self
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Process a message returning zero, one or more messages
    ///
    /// The "process" function of the JS module is passed 3 arguments
    /// - the current timestamp
    /// - the message to be transformed
    /// - the filter config (as configured for the pipeline stage, possibly updated by update_config messages)
    ///
    /// The returned value is expected to be an array of messages.
    pub async fn process(
        &self,
        js: &JsRuntime,
        timestamp: &DateTime,
        message: &Message,
    ) -> Result<Vec<Message>, FilterError> {
        debug!(target: "MAPPING", "{}: process({timestamp:?}, {message:?})", self.module_name());
        if self.no_js_process {
            return Ok(vec![message.clone()]);
        }

        let input = vec![
            timestamp.clone().into(),
            message.clone().into(),
            self.config.clone(),
        ];
        js.call_function(&self.module_name(), "process", input)
            .await
            .map_err(pipeline::error_from_js)?
            .try_into()
    }

    /// Update the filter config using a metadata message
    ///
    /// The "update_config" function of the JS module is passed 2 arguments
    /// - the message
    /// - the current filter config
    ///
    /// The value returned by this function is used as the updated filter config
    pub async fn update_config(
        &mut self,
        js: &JsRuntime,
        message: &Message,
    ) -> Result<(), FilterError> {
        debug!(target: "MAPPING", "{}: update_config({message:?})", self.module_name());
        if self.no_js_update_config {
            return Ok(());
        }

        let input = vec![message.clone().into(), self.config.clone()];
        let config = js
            .call_function(&self.module_name(), "update_config", input)
            .await
            .map_err(pipeline::error_from_js)?;
        self.config = config;
        Ok(())
    }

    /// Trigger the tick function of the JS module
    ///
    /// The "tick" function is passed 2 arguments
    /// - the current timestamp
    /// - the current filter config
    ///
    /// Return zero, one or more messages
    pub async fn tick(
        &self,
        js: &JsRuntime,
        timestamp: &DateTime,
    ) -> Result<Vec<Message>, FilterError> {
        if self.no_js_tick {
            return Ok(vec![]);
        }
        if !timestamp.tick_now(self.tick_every_seconds) {
            return Ok(vec![]);
        }
        debug!(target: "MAPPING", "{}: tick({timestamp:?})", self.module_name());
        let input = vec![timestamp.clone().into(), self.config.clone()];
        js.call_function(&self.module_name(), "tick", input)
            .await
            .map_err(pipeline::error_from_js)?
            .try_into()
    }
}

impl From<Message> for JsonValue {
    fn from(value: Message) -> Self {
        JsonValue(value.json())
    }
}

impl From<DateTime> for JsonValue {
    fn from(value: DateTime) -> Self {
        JsonValue(value.json())
    }
}

impl TryFrom<serde_json::Value> for Message {
    type Error = FilterError;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        let message = serde_json::from_value(value)
            .with_context(|| "Couldn't extract message payload and topic")?;
        Ok(message)
    }
}

impl TryFrom<JsonValue> for Message {
    type Error = FilterError;

    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        Message::try_from(value.0)
    }
}

impl TryFrom<JsonValue> for Vec<Message> {
    type Error = FilterError;

    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        match value.0 {
            serde_json::Value::Array(array) => array.into_iter().map(Message::try_from).collect(),
            serde_json::Value::Object(map) => {
                Message::try_from(serde_json::Value::Object(map)).map(|message| vec![message])
            }
            _ => Err(anyhow::anyhow!("Filters are expected to return an array of messages").into()),
        }
    }
}

struct JsonValueRef<'a>(&'a serde_json::Value);

impl<'js> IntoJs<'js> for JsonValue {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        JsonValueRef(&self.0).into_js(ctx)
    }
}

impl<'js> IntoJs<'js> for &JsonValue {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        JsonValueRef(&self.0).into_js(ctx)
    }
}

impl<'a, 'js> IntoJs<'js> for JsonValueRef<'a> {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        match self.0 {
            serde_json::Value::Null => Ok(Value::new_null(ctx.clone())),
            serde_json::Value::Bool(value) => Ok(Value::new_bool(ctx.clone(), *value)),
            serde_json::Value::Number(value) => {
                if let Some(n) = value.as_i64() {
                    if let Ok(n) = i32::try_from(n) {
                        return Ok(Value::new_int(ctx.clone(), n));
                    }
                }
                if let Some(f) = value.as_f64() {
                    return Ok(Value::new_float(ctx.clone(), f));
                }
                let nan = rquickjs::String::from_str(ctx.clone(), "NaN")?;
                Ok(nan.into_value())
            }
            serde_json::Value::String(value) => {
                let string = rquickjs::String::from_str(ctx.clone(), value)?;
                Ok(string.into_value())
            }
            serde_json::Value::Array(values) => {
                let array = rquickjs::Array::new(ctx.clone())?;
                for (i, value) in values.iter().enumerate() {
                    array.set(i, JsonValueRef(value))?;
                }
                Ok(array.into_value())
            }
            serde_json::Value::Object(values) => {
                let object = rquickjs::Object::new(ctx.clone())?;
                for (key, value) in values.into_iter() {
                    object.set(key, JsonValueRef(value))?;
                }
                Ok(object.into_value())
            }
        }
    }
}

impl<'js> FromJs<'js> for JsonValue {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<Self> {
        JsonValue::from_js_value(value)
    }
}

impl JsonValue {
    fn from_js_value(value: Value<'_>) -> rquickjs::Result<Self> {
        if let Some(b) = value.as_bool() {
            return Ok(JsonValue(serde_json::Value::Bool(b)));
        }
        if let Some(n) = value.as_int() {
            return Ok(JsonValue(serde_json::Value::Number(n.into())));
        }
        if let Some(n) = value.as_float() {
            let js_n = serde_json::Number::from_f64(n)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null);
            return Ok(JsonValue(js_n));
        }
        if let Some(string) = value.as_string() {
            return Ok(JsonValue(serde_json::Value::String(string.to_string()?)));
        }
        if let Some(array) = value.as_array() {
            let array: rquickjs::Result<Vec<JsonValue>> = array.iter().collect();
            let array = array?.into_iter().map(|v| v.0).collect();
            return Ok(JsonValue(serde_json::Value::Array(array)));
        }
        if let Some(object) = value.as_object() {
            let mut js_object = serde_json::Map::new();
            for key in object.keys::<String>().flatten() {
                if let Ok(JsonValue(v)) = object.get(&key) {
                    js_object.insert(key, v.clone());
                }
            }
            return Ok(JsonValue(serde_json::Value::Object(js_object)));
        }

        Ok(JsonValue(serde_json::Value::Null))
    }

    pub(crate) fn display(value: Value<'_>) -> String {
        let json = JsonValue::from_js_value(value).unwrap_or_default();
        serde_json::to_string_pretty(&json.0).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn identity_filter() {
        let script = "export function process(t,msg) { return [msg]; };";
        let mut runtime = JsRuntime::try_new().await.unwrap();
        let filter = JsFilter::new("id.toml".into(), 1, "id.js".into());
        runtime.load_js(filter.module_name(), script).await.unwrap();

        let input = Message::new("te/main/device///m/", "hello world");
        let output = input.clone();
        assert_eq!(
            filter
                .process(&runtime, &DateTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn error_filter() {
        let script = r#"export function process(t,msg) { throw new Error("Cannot process that message"); };"#;
        let mut runtime = JsRuntime::try_new().await.unwrap();
        let mut filter = JsFilter::new("err.toml".into(), 1, "err.js".into());
        filter.no_js_process = false;
        runtime.load_js(filter.module_name(), script).await.unwrap();

        let input = Message::new("te/main/device///m/", "hello world");
        let error = filter
            .process(&runtime, &DateTime::now(), &input)
            .await
            .unwrap_err();
        eprintln!("{:?}", error);
        assert!(error.to_string().contains("Cannot process that message"));
    }

    #[tokio::test]
    async fn collectd_filter() {
        let script = r#"
export function process (timestamp, message, config) {
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
        let mut runtime = JsRuntime::try_new().await.unwrap();
        let mut filter = JsFilter::new("collectd.toml".into(), 1, "collectd.js".into());
        filter.no_js_process = false;
        runtime.load_js(filter.module_name(), script).await.unwrap();

        let input = Message::new(
            "collectd/h/memory/percent-used",
            "1748440192.104:19.9289468288182",
        );
        let output = Message::new(
            "te/device/main///m/collectd",
            r#"{"time": 1748440192.104, "memory": {"percent-used": 19.9289468288182}}"#,
        );
        assert_eq!(
            filter
                .process(&runtime, &DateTime::now(), &input)
                .await
                .unwrap(),
            vec![output]
        );
    }
}
