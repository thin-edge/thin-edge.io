use crate::pipeline;
use crate::pipeline::FilterError;
use crate::LoadError;
use rquickjs::Ctx;
use rquickjs::FromJs;
use rquickjs::IntoJs;
use rquickjs::Object;
use rquickjs::Value;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
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
}

impl JsFilter {
    pub async fn process(
        &self,
        js: &JsRuntime,
        timestamp: OffsetDateTime,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, FilterError> {
        debug!(target: "MAPPING", "{}: process({timestamp}, {message:?})", self.path.display());
        let timestamp = DateTime::try_from(timestamp)?;
        let message = Message::try_from(message)?;
        let input = (timestamp, message);
        let output: Vec<Message> = js
            .call_function(&self, "process", input)
            .await
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
        Ok(JsFilter { path })
    }

    pub fn loaded_module(&self, path: PathBuf) -> Result<JsFilter, LoadError> {
        match self.modules.get(&path) {
            None => Err(LoadError::ScriptNotLoaded { path }),
            Some(_) => Ok(JsFilter { path }),
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
        for<'a> Ret: rquickjs::FromJs<'a> + Send + 'a,
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

fn error_from_js(err: LoadError) -> FilterError {
    FilterError::IncorrectSetting(format!("{}", err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_mqtt_ext::Topic;

    #[tokio::test]
    async fn identity_filter() {
        let script = "export function process(t,msg) { return [msg]; };";
        let mut runtime = JsRuntime::try_new().await.unwrap();
        let filter = runtime.load_js("id.js", script).unwrap();

        let topic = Topic::new_unchecked("te/main/device///m/");
        let input = MqttMessage::new(&topic, "hello world");
        let output = input.clone();
        assert_eq!(
            filter
                .process(&runtime, OffsetDateTime::now_utc(), &input)
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

        let topic = Topic::new_unchecked("te/main/device///m/");
        let input = MqttMessage::new(&topic, "hello world");
        let error = filter
            .process(&runtime, OffsetDateTime::now_utc(), &input)
            .await
            .unwrap_err();
        eprintln!("{:?}", error);
        assert!(error.to_string().contains("Exception generated by QuickJS"));
    }
}
