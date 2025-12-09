use crate::js_value::JsonValue;
use crate::FlowError;
use crate::LoadError;
use crate::Message;
use std::collections::HashMap;
use std::time::SystemTime;

mod add_timestamp;
mod set_topic;

pub trait Transformer: Send + Sync + 'static {
    fn name(&self) -> &str;

    fn on_message(
        &self,
        timestamp: SystemTime,
        message: &Message,
        config: &JsonValue,
    ) -> Result<Vec<Message>, FlowError>;

    fn is_periodic(&self) -> bool {
        false
    }

    fn on_interval(
        &self,
        _timestamp: SystemTime,
        _config: &JsonValue,
    ) -> Result<Vec<Message>, FlowError> {
        Ok(vec![])
    }
}

pub trait TransformerBuilder: Send + Sync + 'static {
    fn new_instance(&self) -> Box<dyn Transformer>;
}

impl<T: Default + Clone + Transformer> TransformerBuilder for T {
    fn new_instance(&self) -> Box<dyn Transformer> {
        Box::new(Self::default().clone())
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
        transformers.register(add_timestamp::AddTimestamp);
        transformers.register(set_topic::SetTopic);
        transformers
    }
}

impl BuiltinTransformers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, prototype: impl TransformerBuilder + Transformer) {
        self.register_builder(prototype.name().to_owned(), prototype);
    }

    pub fn register_builder(&mut self, name: impl ToString, transformer: impl TransformerBuilder) {
        self.transformers
            .insert(name.to_string(), Box::new(transformer));
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
    use crate::js_runtime::JsRuntime;
    use crate::steps::FlowStep;
    use std::time::Duration;

    #[tokio::test]
    async fn adding_timestamp() {
        let step = r#"
builtin = "add-timestamp"
config = { property = "time", format = "rfc-3339" }
        "#;
        let transformers = BuiltinTransformers::new();
        let (runtime, step) = step_instance(&transformers, step).await;

        let datetime = SystemTime::UNIX_EPOCH + Duration::from_secs(1763050414);
        let input = Message::new("clock", "{}");
        let output = Message::new("clock", r#"{"time":1763050414}"#.to_string());
        assert_eq!(
            step.on_message(&runtime, datetime, &input).await.unwrap(),
            vec![output]
        );
    }

    async fn step_instance(
        transformers: &BuiltinTransformers,
        config: &str,
    ) -> (JsRuntime, FlowStep) {
        let mut runtime = JsRuntime::try_new().await.unwrap();
        let step = toml::from_str::<StepConfig>(config)
            .unwrap()
            .compile(
                transformers,
                &mut runtime,
                "/tmp".into(),
                0,
                "test-flow".into(),
            )
            .await
            .unwrap();
        (runtime, step)
    }
}
