use crate::pipeline;
use crate::pipeline::DateTime;
use crate::pipeline::FilterError;
use crate::pipeline::Message;
use crate::LoadError;
use rquickjs::Ctx;
use rquickjs::FromJs;
use rquickjs::IntoJs;
use rquickjs::Object;
use rquickjs::Value;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use tracing::debug;

#[derive(Clone)]
pub struct JsFilter {
    path: PathBuf,
    config: JsonValue,
}

#[derive(Clone, Default)]
pub struct JsonValue(serde_json::Value);

impl JsFilter {
    pub fn new(path: PathBuf) -> Self {
        JsFilter {
            path,
            config: JsonValue::default(),
        }
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
        debug!(target: "MAPPING", "{}: process({timestamp:?}, {message:?})", self.path.display());
        let input = (timestamp.clone(), message.clone(), self.config.clone());
        js.call_function(&self, "process", input)
            .await
            .map_err(pipeline::error_from_js)
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
        debug!(target: "MAPPING", "{}: update_config({message:?})", self.path.display());
        let input = (message.clone(), self.config.clone());
        let config = js
            .call_function(&self, "update_config", input)
            .await
            .map_err(pipeline::error_from_js)?;
        self.config = config;
        Ok(())
    }

    pub fn tick(&self, _js: &JsRuntime, timestamp: &DateTime) -> Result<Vec<Message>, FilterError> {
        debug!(target: "MAPPING", "{}: tick({timestamp:?})", self.path.display());
        Ok(vec![])
    }
}

pub struct JsRuntime {
    context: rquickjs::AsyncContext,
    modules: HashMap<PathBuf, Vec<u8>>,
}

impl JsRuntime {
    pub async fn try_new() -> Result<Self, LoadError> {
        let runtime = rquickjs::AsyncRuntime::new()?;
        let context = rquickjs::AsyncContext::full(&runtime).await?;
        let modules = HashMap::new();
        Ok(JsRuntime { context, modules })
    }

    pub async fn load_file(&mut self, path: impl AsRef<Path>) -> Result<JsFilter, LoadError> {
        let path = path.as_ref();
        let source = tokio::fs::read_to_string(path).await?;
        self.load_js(path, source)
    }

    pub fn load_js(
        &mut self,
        path: impl AsRef<Path>,
        source: impl Into<Vec<u8>>,
    ) -> Result<JsFilter, LoadError> {
        let path = path.as_ref().to_path_buf();
        self.modules.insert(path.clone(), source.into());
        Ok(JsFilter::new(path))
    }

    pub fn loaded_module(&self, path: PathBuf) -> Result<JsFilter, LoadError> {
        match self.modules.get(&path) {
            None => Err(LoadError::ScriptNotLoaded { path }),
            Some(_) => Ok(JsFilter::new(path)),
        }
    }

    pub async fn call_function<Args, Ret>(
        &self,
        module: &JsFilter,
        function: &str,
        args: Args,
    ) -> Result<Ret, LoadError>
    where
        for<'a> Args: rquickjs::function::IntoArgs<'a> + Send + 'a,
        for<'a> Ret: FromJs<'a> + Send + 'a,
    {
        let Some(source) = self.modules.get(&module.path) else {
            return Err(LoadError::ScriptNotLoaded {
                path: module.path.clone(),
            });
        };

        let name = module.path.display().to_string();

        rquickjs::async_with!(self.context => |ctx| {
            let m = rquickjs::Module::declare(ctx, name, source.clone())?;
            let (m,p) = m.eval()?;
            let () = p.finish()?;

            let f: rquickjs::Value = m.get(function)?;
            let f = rquickjs::Function::from_value(f)?;
            let r = f.call(args)?;
            Ok(r)
        })
        .await
    }
}

impl<'js> FromJs<'js> for Message {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<Self> {
        match value.as_object() {
            None => Ok(Message {
                topic: "".to_string(),
                payload: "".to_string(),
            }),
            Some(object) => Ok(Message {
                topic: object.get("topic")?,
                payload: object.get("payload")?,
            }),
        }
    }
}

impl<'js> IntoJs<'js> for Message {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        let msg = Object::new(ctx.clone())?;
        msg.set("topic", self.topic)?;
        msg.set("payload", self.payload)?;
        Ok(Value::from_object(msg))
    }
}

impl<'js> IntoJs<'js> for DateTime {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        let msg = Object::new(ctx.clone())?;
        msg.set("topic", self.seconds)?;
        msg.set("payload", self.nanoseconds)?;
        Ok(Value::from_object(msg))
    }
}

impl<'js> IntoJs<'js> for JsonValue {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        match self.0 {
            serde_json::Value::Null => Ok(Value::new_null(ctx.clone())),
            serde_json::Value::Bool(value) => Ok(Value::new_bool(ctx.clone(), value)),
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
                let string = rquickjs::String::from_str(ctx.clone(), &value)?;
                Ok(string.into_value())
            }
            serde_json::Value::Array(values) => {
                let array = rquickjs::Array::new(ctx.clone())?;
                for (i, value) in values.into_iter().enumerate() {
                    array.set(i, JsonValue(value))?;
                }
                Ok(array.into_value())
            }
            serde_json::Value::Object(values) => {
                let object = rquickjs::Object::new(ctx.clone())?;
                for (key, value) in values.into_iter() {
                    object.set(key, JsonValue(value))?;
                }
                Ok(object.into_value())
            }
        }
    }
}

impl<'js> FromJs<'js> for JsonValue {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<Self> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn identity_filter() {
        let script = "export function process(t,msg) { return [msg]; };";
        let mut runtime = JsRuntime::try_new().await.unwrap();
        let filter = runtime.load_js("id.js", script).unwrap();

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
        let filter = runtime.load_js("err.js", script).unwrap();

        let input = Message::new("te/main/device///m/", "hello world");
        let error = filter
            .process(&runtime, &DateTime::now(), &input)
            .await
            .unwrap_err();
        eprintln!("{:?}", error);
        assert!(error.to_string().contains("Exception generated by QuickJS"));
    }
}
