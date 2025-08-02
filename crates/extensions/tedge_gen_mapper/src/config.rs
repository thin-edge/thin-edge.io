use crate::flow::Flow;
use crate::flow::FlowInput;
use crate::flow::FlowStep;
use crate::js_runtime::JsRuntime;
use crate::js_script::JsScript;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use serde::Deserialize;
use serde_json::Value;
use std::fmt::Debug;
use std::path::Path;
use tedge_mqtt_ext::TopicFilter;

#[derive(Deserialize)]
pub struct FlowConfig {
    input: InputConfig,
    steps: Vec<StepConfig>,
}

#[derive(Deserialize)]
pub struct StepConfig {
    script: ScriptSpec,

    #[serde(default)]
    config: Option<Value>,

    #[serde(default)]
    tick_every_seconds: u64,

    #[serde(default)]
    meta_topics: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum ScriptSpec {
    JavaScript(Utf8PathBuf),
}

#[derive(Deserialize)]
pub enum InputConfig {
    #[serde(rename = "mqtt")]
    Mqtt { topics: Vec<String> },
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Not a valid MQTT topic filter: {0}")]
    IncorrectTopicFilter(String),

    #[error(transparent)]
    LoadError(#[from] LoadError),
}

impl FlowConfig {
    pub fn from_step(script: Utf8PathBuf) -> Self {
        let input_topic = "#".to_string();
        let step = StepConfig {
            script: ScriptSpec::JavaScript(script),
            config: None,
            tick_every_seconds: 0,
            meta_topics: vec![],
        };
        Self {
            input: InputConfig::Mqtt {
                topics: vec![input_topic],
            },
            steps: vec![step],
        }
    }

    pub async fn compile(
        self,
        js_runtime: &mut JsRuntime,
        config_dir: &Path,
        source: Utf8PathBuf,
    ) -> Result<Flow, ConfigError> {
        let input = self.input.try_into()?;
        let mut steps = vec![];
        for (i, step) in self.steps.into_iter().enumerate() {
            let mut step = step.compile(config_dir, i, &source).await?;
            js_runtime.load_script(&mut step.script).await?;
            step.check(&source);
            step.fix();
            steps.push(step);
        }
        Ok(Flow {
            input,
            steps,
            source,
        })
    }
}

impl StepConfig {
    pub async fn compile(
        self,
        config_dir: &Path,
        index: usize,
        flow: &Utf8Path,
    ) -> Result<FlowStep, ConfigError> {
        let path = match self.script {
            ScriptSpec::JavaScript(path) if path.is_absolute() => path.into(),
            ScriptSpec::JavaScript(path) if path.starts_with(config_dir) => path.into(),
            ScriptSpec::JavaScript(path) => config_dir.join(path),
        };
        let script = JsScript::new(flow.to_owned().into(), index, path)
            .with_config(self.config)
            .with_tick_every_seconds(self.tick_every_seconds);
        let config_topics = topic_filters(self.meta_topics)?;
        Ok(FlowStep {
            script,
            config_topics,
        })
    }
}

impl TryFrom<InputConfig> for FlowInput {
    type Error = ConfigError;

    fn try_from(input: InputConfig) -> Result<Self, Self::Error> {
        match input {
            InputConfig::Mqtt { topics } => Ok(FlowInput::MQTT {
                topics: topic_filters(topics)?,
            }),
        }
    }
}
fn topic_filters(patterns: Vec<String>) -> Result<TopicFilter, ConfigError> {
    let mut topics = TopicFilter::empty();
    for pattern in patterns {
        topics
            .add(pattern.as_str())
            .map_err(|_| ConfigError::IncorrectTopicFilter(pattern.clone()))?;
    }
    Ok(topics)
}
