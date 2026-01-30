use crate::config::ConfigError;
use crate::js_value::JsonValue;
use crate::FlowContextHandle;
use crate::FlowError;
use crate::LoadError;
use crate::Message;
use std::collections::HashMap;
use std::time::SystemTime;

mod add_timestamp;
mod ignore_topics;
mod limit_payload_size;
mod set_topic;
mod skip_mosquitto_health_status;
mod update_context;

pub trait Transformer: Send + Sync + 'static {
    fn name(&self) -> &str;

    fn set_config(&mut self, config: JsonValue) -> Result<(), ConfigError>;

    fn on_message(
        &mut self,
        timestamp: SystemTime,
        message: &Message,
        context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError>;

    fn is_periodic(&self) -> bool {
        false
    }

    fn on_interval(
        &mut self,
        _timestamp: SystemTime,
        _context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        Ok(vec![])
    }
}

pub trait TransformerBuilder: Send + Sync + 'static {
    fn new_instance(&self) -> Box<dyn Transformer>;
}

impl<T: Clone + Transformer> TransformerBuilder for T {
    fn new_instance(&self) -> Box<dyn Transformer> {
        Box::new(self.clone())
    }
}

pub struct BuiltinTransformers {
    transformers: HashMap<String, Box<dyn TransformerBuilder>>,
}

impl Default for BuiltinTransformers {
    fn default() -> Self {
        let mut transformers = BuiltinTransformers {
            transformers: HashMap::default(),
        };
        transformers.register(add_timestamp::AddTimestamp::default());
        transformers.register(limit_payload_size::LimitPayloadSize::default());
        transformers.register(ignore_topics::IgnoreTopics::default());
        transformers.register(set_topic::SetTopic::default());
        transformers.register(skip_mosquitto_health_status::SkipMosquittoHealthStatus);
        transformers.register(update_context::UpdateContext::default());
        transformers
    }
}

impl BuiltinTransformers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, prototype: impl TransformerBuilder + Transformer) {
        self.transformers
            .insert(prototype.name().to_owned(), Box::new(prototype));
    }

    pub fn new_instance(&self, name: &str) -> Result<Box<dyn Transformer>, LoadError> {
        let Some(builder) = self.transformers.get(name) else {
            return Err(LoadError::UnknownTransformer { name: name.into() });
        };
        Ok(builder.new_instance())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StepConfig;
    use crate::js_lib::kv_store::FlowContextHandle;
    use crate::js_runtime::JsRuntime;
    use crate::steps::FlowStep;
    use serde_json::json;
    use std::time::Duration;

    #[tokio::test]
    async fn adding_unix_timestamp() {
        let step = r#"
builtin = "add-timestamp"
config = { property = "time", format = "unix" }
        "#;
        let transformers = BuiltinTransformers::new();
        let (runtime, mut step) = step_instance(&transformers, step).await;

        let datetime = SystemTime::UNIX_EPOCH + Duration::from_secs(1763050414);
        let input = Message::new("clock", "{}");
        let output = Message::new("clock", r#"{"time":1763050414.0}"#.to_string());
        assert_eq!(
            step.on_message(&runtime, datetime, &input).await.unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn adding_rfc3339_timestamp() {
        let step = r#"
builtin = "add-timestamp"
config = { property = "time", format = "rfc-3339" }
        "#;
        let transformers = BuiltinTransformers::new();
        let (runtime, mut step) = step_instance(&transformers, step).await;

        let datetime = SystemTime::UNIX_EPOCH + Duration::from_secs(1763050414);
        let input = Message::new("clock", "{}");
        let output = Message::new("clock", r#"{"time":"2025-11-13T16:13:34Z"}"#.to_string());
        assert_eq!(
            step.on_message(&runtime, datetime, &input).await.unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn reformating_timestamp_as_rfc3339() {
        let step = r#"
builtin = "add-timestamp"
config = { property = "time", format = "rfc-3339", reformat = true }
        "#;
        let transformers = BuiltinTransformers::new();
        let (runtime, mut step) = step_instance(&transformers, step).await;

        let datetime = SystemTime::UNIX_EPOCH + Duration::from_secs(1763050414);
        let input = Message::new("clock", r#"{"time":1765555467}"#);
        let output = Message::new("clock", r#"{"time":"2025-12-12T16:04:27Z"}"#.to_string());
        assert_eq!(
            step.on_message(&runtime, datetime, &input).await.unwrap(),
            vec![output]
        );
    }

    #[tokio::test]
    async fn updating_the_context() {
        let step = r#"
builtin = "update-context"
config = { topics = ["units/#"] }
"#;
        let transformers = BuiltinTransformers::new();
        let (runtime, mut step) = step_instance(&transformers, step).await;
        let datetime = SystemTime::UNIX_EPOCH + Duration::from_secs(1763050414);

        // Updating the context
        let input = Message::new("units/temperature", r#""°C""#);
        let expected = json!("°C");
        assert_eq!(
            step.on_message(&runtime, datetime, &input).await.unwrap(),
            vec![]
        );
        assert_eq!(
            runtime.context_handle().get_value("units/temperature"),
            expected.into()
        );

        // Clearing the context
        let input = Message::new("units/temperature", "");
        assert_eq!(
            step.on_message(&runtime, datetime, &input).await.unwrap(),
            vec![]
        );
        assert_eq!(
            runtime.context_handle().get_value("units/temperature"),
            JsonValue::Null
        );

        // Filtering out messages with a non-relevant topic
        let input = Message::new("must/not/be/stored/in/the/context", "Garbage");
        assert_eq!(
            step.on_message(&runtime, datetime, &input).await.unwrap(),
            vec![input]
        );
    }

    async fn step_instance(
        transformers: &BuiltinTransformers,
        config: &str,
    ) -> (JsRuntime, FlowStep) {
        let context = FlowContextHandle::default();
        let mut runtime = JsRuntime::try_new(context).await.unwrap();
        let step = toml::from_str::<StepConfig>(config)
            .unwrap()
            .compile(transformers, &mut runtime, 0, "test-flow".into())
            .await
            .unwrap();
        (runtime, step)
    }
}
