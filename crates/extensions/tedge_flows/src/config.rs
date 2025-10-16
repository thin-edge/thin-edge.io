use crate::flow::Flow;
use crate::flow::FlowInput;
use crate::flow::FlowOutput;
use crate::flow::FlowStep;
use crate::input_source::CommandFlowInput;
use crate::input_source::FileFlowInput;
use crate::input_source::PollingSource;
use crate::js_runtime::JsRuntime;
use crate::js_script::JsScript;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use serde::Deserialize;
use serde_json::Value;
use std::fmt::Debug;
use std::time::Duration;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;

#[derive(Deserialize)]
pub struct FlowConfig {
    input: InputConfig,
    #[serde(default)]
    steps: Vec<StepConfig>,
    #[serde(default = "default_output")]
    output: OutputConfig,
    #[serde(default = "default_errors")]
    errors: OutputConfig,
}

#[derive(Deserialize)]
pub struct StepConfig {
    script: ScriptSpec,

    #[serde(default)]
    config: Option<Value>,

    #[serde(default)]
    #[serde(deserialize_with = "parse_human_duration")]
    interval: Duration,

    #[serde(default)]
    meta_topics: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum ScriptSpec {
    JavaScript(Utf8PathBuf),
}

#[derive(Clone, Deserialize)]
pub enum InputConfig {
    #[serde(rename = "mqtt")]
    Mqtt { topics: Vec<String> },

    #[serde(rename = "file")]
    File {
        path: Utf8PathBuf,

        /// Default to path
        topic: Option<String>,

        #[serde(default)]
        #[serde(deserialize_with = "parse_optional_human_duration")]
        interval: Option<Duration>,
    },

    #[serde(rename = "process")]
    Process {
        command: String,

        /// Default to command
        topic: Option<String>,

        #[serde(default)]
        #[serde(deserialize_with = "parse_optional_human_duration")]
        interval: Option<Duration>,
    },
}

#[derive(Deserialize)]
pub enum OutputConfig {
    #[serde(rename = "mqtt")]
    Mqtt { topic: Option<String> },

    #[serde(rename = "file")]
    File { path: Utf8PathBuf },
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Not a valid MQTT topic: {0}")]
    IncorrectTopic(String),

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
            interval: Duration::default(),
            meta_topics: vec![],
        };
        Self {
            input: InputConfig::Mqtt {
                topics: vec![input_topic],
            },
            steps: vec![step],
            output: default_output(),
            errors: default_errors(),
        }
    }

    pub async fn compile(
        self,
        js_runtime: &mut JsRuntime,
        config_dir: &Utf8Path,
        source: Utf8PathBuf,
    ) -> Result<Flow, ConfigError> {
        let input = self.input.clone().try_into()?;
        let input_source = self.input.try_into()?;
        let output = self.output.try_into()?;
        let errors = self.errors.try_into()?;
        let mut steps = vec![];
        for (i, step) in self.steps.into_iter().enumerate() {
            let mut step = step.compile(config_dir, i, &source).await?;
            js_runtime.load_script(&mut step.script).await?;
            step.check(&source);
            step.fix();
            step.script.init_next_execution();
            steps.push(step);
        }
        Ok(Flow {
            input,
            input_source,
            steps,
            output,
            errors,
            source,
        })
    }
}

impl StepConfig {
    pub async fn compile(
        self,
        config_dir: &Utf8Path,
        index: usize,
        flow: &Utf8Path,
    ) -> Result<FlowStep, ConfigError> {
        let path = match self.script {
            ScriptSpec::JavaScript(path) if path.is_absolute() => path,
            ScriptSpec::JavaScript(path) if path.starts_with(config_dir) => path,
            ScriptSpec::JavaScript(path) => config_dir.join(path),
        };
        let script = JsScript::new(flow.to_owned(), index, path)
            .with_config(self.config)
            .with_interval(self.interval);
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
        Ok(match input {
            InputConfig::Mqtt { topics } => FlowInput::Mqtt {
                topics: topic_filters(topics)?,
            },

            InputConfig::File { path, interval, .. } if interval.is_none() => {
                FlowInput::File { path }
            }
            InputConfig::File { topic, path, .. } => {
                let topic = topic.unwrap_or_else(|| path.to_string());
                FlowInput::OnInterval { topic }
            }

            InputConfig::Process {
                command, interval, ..
            } if interval.is_none() => FlowInput::Process { command },
            InputConfig::Process { topic, command, .. } => {
                let topic = topic.unwrap_or(command);
                FlowInput::OnInterval { topic }
            }
        })
    }
}

impl TryFrom<InputConfig> for Option<Box<dyn PollingSource>> {
    type Error = ConfigError;

    fn try_from(value: InputConfig) -> Result<Self, Self::Error> {
        match value {
            InputConfig::Mqtt { .. } => Ok(None),

            InputConfig::File { interval, .. } if interval.is_none() => Ok(None),
            InputConfig::File {
                topic,
                path,
                interval,
            } => {
                let topic = topic.unwrap_or_else(|| path.to_string());
                Ok(Some(Box::new(FileFlowInput::new(topic, path, interval))))
            }

            InputConfig::Process { interval, .. } if interval.is_none() => Ok(None),
            InputConfig::Process {
                topic,
                command,
                interval,
            } => {
                let topic = topic.unwrap_or_else(|| command.clone());
                Ok(Some(Box::new(CommandFlowInput::new(
                    topic, command, interval,
                ))))
            }
        }
    }
}

impl TryFrom<OutputConfig> for FlowOutput {
    type Error = ConfigError;

    fn try_from(input: OutputConfig) -> Result<Self, Self::Error> {
        Ok(match input {
            OutputConfig::Mqtt { topic } => FlowOutput::Mqtt {
                topic: topic.map(into_topic).transpose()?,
            },
            OutputConfig::File { path } => FlowOutput::File { path },
        })
    }
}

fn into_topic(name: String) -> Result<Topic, ConfigError> {
    Topic::new(&name).map_err(|_| ConfigError::IncorrectTopic(name))
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

fn parse_human_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    humantime::parse_duration(&value).map_err(|_| serde::de::Error::custom("Invalid duration"))
}

fn parse_optional_human_duration<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    if value.trim().is_empty() {
        Ok(None)
    } else {
        humantime::parse_duration(&value)
            .map_err(|_| serde::de::Error::custom("Invalid duration"))
            .map(Some)
    }
}

fn default_output() -> OutputConfig {
    OutputConfig::Mqtt { topic: None }
}

fn default_errors() -> OutputConfig {
    OutputConfig::Mqtt {
        topic: Some("te/error".to_string()),
    }
}
