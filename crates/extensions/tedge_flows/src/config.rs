use crate::flow::Flow;
use crate::flow::FlowInput;
use crate::flow::FlowOutput;
use crate::js_runtime::JsRuntime;
use crate::js_script::JsScript;
use crate::steps::FlowStep;
use crate::transformers::BuiltinTransformers;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Debug;
use std::time::Duration;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tokio::fs::read_dir;
use tokio::fs::read_to_string;
use tracing::error;
use tracing::info;

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
    #[serde(flatten)]
    step: StepSpec,

    #[serde(default)]
    config: Option<Value>,

    #[serde(default)]
    #[serde(deserialize_with = "parse_optional_human_duration")]
    interval: Option<Duration>,
}

#[derive(Deserialize)]
pub enum StepSpec {
    #[serde(rename = "builtin")]
    Transformer(String),

    #[serde(rename = "script")]
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

    #[serde(rename = "context")]
    Context,
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Not a valid MQTT topic: {0}")]
    IncorrectTopic(String),

    #[error("Not a valid MQTT topic filter: {0}")]
    IncorrectTopicFilter(String),

    #[error(transparent)]
    LoadError(#[from] LoadError),

    #[error("Not a valid step configuration: {0}")]
    IncorrectSetting(String),
}

impl FlowConfig {
    pub async fn load_all_flows(config_dir: &Utf8Path) -> HashMap<Utf8PathBuf, FlowConfig> {
        let mut flows = HashMap::new();
        let Ok(mut entries) = read_dir(config_dir).await.map_err(
            |err| error!(target: "flows", "Failed to read flows from {config_dir}: {err}"),
        ) else {
            return flows;
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let Some(path) = Utf8Path::from_path(&entry.path()).map(|p| p.to_path_buf()) else {
                error!(target: "flows", "Skipping non UTF8 path: {}", entry.path().display());
                continue;
            };
            if let Ok(file_type) = entry.file_type().await {
                if file_type.is_file() {
                    if let Some("toml") = path.extension() {
                        info!(target: "flows", "Loading flow: {path}");
                        if let Some(flow) = FlowConfig::load_single_flow(&path).await {
                            flows.insert(path.clone(), flow);
                        }
                    }
                }
            }
        }
        flows
    }

    pub async fn load_single_flow(flow: &Utf8Path) -> Option<FlowConfig> {
        match FlowConfig::load_flow(flow).await {
            Ok(flow) => Some(flow),
            Err(err) => {
                error!(target: "flows", "Failed to load flow {flow}: {err}");
                None
            }
        }
    }

    pub fn wrap_script_into_flow(script: &Utf8Path) -> FlowConfig {
        FlowConfig::from_step(script.to_owned())
    }

    async fn load_flow(path: &Utf8Path) -> Result<FlowConfig, LoadError> {
        let specs = read_to_string(path)
            .await
            .map_err(|err| LoadError::from_io(err, path))?;
        let flow: FlowConfig = toml::from_str(&specs)?;
        Ok(flow)
    }

    pub fn from_step(script: Utf8PathBuf) -> Self {
        let input_topic = "#".to_string();
        let step = StepConfig {
            step: StepSpec::JavaScript(script),
            config: None,
            interval: None,
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
        rs_transformers: &BuiltinTransformers,
        js_runtime: &mut JsRuntime,
        config_dir: &Utf8Path,
        source: Utf8PathBuf,
    ) -> Result<Flow, ConfigError> {
        let input = self.input.try_into()?;
        let output = self.output.try_into()?;
        let errors = self.errors.try_into()?;
        let mut steps = vec![];
        for (i, step) in self.steps.into_iter().enumerate() {
            let step = step
                .compile(rs_transformers, js_runtime, config_dir, i, &source)
                .await?;
            steps.push(step);
        }
        Ok(Flow {
            input,
            steps,
            output,
            errors,
            source,
        })
    }
}

impl StepConfig {
    pub async fn compile(
        &self,
        rs_transformers: &BuiltinTransformers,
        js_runtime: &mut JsRuntime,
        config_dir: &Utf8Path,
        index: usize,
        flow: &Utf8Path,
    ) -> Result<FlowStep, ConfigError> {
        let step = match &self.step {
            StepSpec::JavaScript(path) => {
                Self::compile_script(js_runtime, config_dir, flow, path, index).await?
            }
            StepSpec::Transformer(name) => {
                Self::instantiate_builtin(rs_transformers, flow, name, index)?
            }
        };
        let step = step
            .with_config(self.config.clone())?
            .with_interval(self.interval, flow.as_str());
        Ok(step)
    }

    async fn compile_script(
        js_runtime: &mut JsRuntime,
        config_dir: &Utf8Path,
        flow: &Utf8Path,
        path: &Utf8Path,
        index: usize,
    ) -> Result<FlowStep, ConfigError> {
        let path = if path.is_absolute() || path.starts_with(config_dir) {
            path.to_owned()
        } else {
            config_dir.join(path)
        };
        let module_name = FlowStep::instance_name(flow, &path, index);
        let mut script = JsScript::new(module_name, flow.to_owned(), path);
        js_runtime.load_script(&mut script).await?;
        Ok(FlowStep::new_script(script))
    }

    fn instantiate_builtin(
        rs_transformers: &BuiltinTransformers,
        flow: &Utf8Path,
        name: &String,
        index: usize,
    ) -> Result<FlowStep, ConfigError> {
        let instance_name = FlowStep::instance_name(flow, name, index);
        let transformer = rs_transformers.new_instance(name)?;
        Ok(FlowStep::new_transformer(instance_name, transformer))
    }
}

impl TryFrom<InputConfig> for FlowInput {
    type Error = ConfigError;
    fn try_from(input: InputConfig) -> Result<Self, Self::Error> {
        Ok(match input {
            InputConfig::Mqtt { topics } => FlowInput::Mqtt {
                topics: topic_filters(topics)?,
            },
            InputConfig::File {
                topic,
                path,
                interval,
            } => {
                let topic = topic.unwrap_or_else(|| path.clone().to_string());
                match interval {
                    Some(interval) if !interval.is_zero() => FlowInput::PollFile {
                        topic,
                        path,
                        interval,
                    },
                    _ => FlowInput::StreamFile { topic, path },
                }
            }
            InputConfig::Process {
                topic,
                command,
                interval,
            } => {
                let topic = topic.unwrap_or_else(|| command.clone());
                match interval {
                    Some(interval) if !interval.is_zero() => FlowInput::PollCommand {
                        topic,
                        command,
                        interval,
                    },
                    _ => FlowInput::StreamCommand { topic, command },
                }
            }
        })
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
            OutputConfig::Context => FlowOutput::Context,
        })
    }
}

fn into_topic(name: String) -> Result<Topic, ConfigError> {
    Topic::new(&name).map_err(|_| ConfigError::IncorrectTopic(name))
}

pub(crate) fn topic_filters<S: AsRef<str> + ToString>(
    patterns: Vec<S>,
) -> Result<TopicFilter, ConfigError> {
    let mut topics = TopicFilter::empty();
    for pattern in patterns {
        topics
            .try_add(pattern.as_ref())
            .map_err(|_| ConfigError::IncorrectTopicFilter(pattern.to_string()))?;
    }
    Ok(topics)
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
