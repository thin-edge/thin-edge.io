use crate::js_filter::JsFilter;
use crate::js_runtime::JsRuntime;
use crate::pipeline::Pipeline;
use crate::pipeline::PipelineInput;
use crate::pipeline::PipelineOutput;
use crate::pipeline::Stage;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use serde::Deserialize;
use serde_json::Value;
use std::fmt::Debug;
use std::path::Path;
use std::time::Duration;
use tedge_mqtt_ext::TopicFilter;

#[derive(Deserialize)]
pub struct PipelineConfig {
    input: InputConfig,

    stages: Vec<StageConfig>,

    #[serde(default)]
    output: OutputConfig,
}

#[derive(Deserialize)]
pub struct StageConfig {
    filter: FilterSpec,

    #[serde(default)]
    config: Option<Value>,

    #[serde(default)]
    tick_every_seconds: u64,

    #[serde(default)]
    meta_topics: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum FilterSpec {
    JavaScript(Utf8PathBuf),
}

#[derive(Deserialize)]
pub enum InputConfig {
    #[serde(rename = "mqtt")]
    Mqtt { topics: Vec<String> },
    #[serde(rename = "db")]
    MeaDB {
        series: String,
        #[serde(deserialize_with = "parse_human_duration")]
        frequency: Duration,
        #[serde(deserialize_with = "parse_human_duration")]
        max_age: Duration,
    },
}

#[derive(Deserialize)]
pub enum OutputConfig {
    #[serde(rename = "mqtt")]
    Mqtt { topics: Vec<String> },
    #[serde(rename = "db")]
    MeaDB { series: String },
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Not a valid MQTT topic filter: {0}")]
    IncorrectTopicFilter(String),

    #[error(transparent)]
    LoadError(#[from] LoadError),
}

impl PipelineConfig {
    pub fn from_filter(filter: Utf8PathBuf) -> Self {
        let input_topic = "#".to_string();
        let output_topic = "#".to_string();
        let stage = StageConfig {
            filter: FilterSpec::JavaScript(filter),
            config: None,
            tick_every_seconds: 1,
            meta_topics: vec![],
        };
        Self {
            input: InputConfig::Mqtt {
                topics: vec![input_topic],
            },
            stages: vec![stage],
            output: OutputConfig::Mqtt {
                topics: vec![output_topic],
            },
        }
    }

    pub async fn compile(
        self,
        js_runtime: &mut JsRuntime,
        config_dir: &Path,
        source: Utf8PathBuf,
    ) -> Result<Pipeline, ConfigError> {
        let input = self.input.try_into()?;
        let output = self.output.try_into()?;
        let mut stages = vec![];
        for (i, stage) in self.stages.into_iter().enumerate() {
            let stage = stage.compile(config_dir, i, &source).await?;
            let filter = &stage.filter;
            js_runtime
                .load_file(filter.module_name(), filter.path())
                .await?;
            stages.push(stage);
        }
        Ok(Pipeline {
            input,
            stages,
            source,
            output,
        })
    }
}

impl StageConfig {
    pub async fn compile(
        self,
        config_dir: &Path,
        index: usize,
        pipeline: &Utf8Path,
    ) -> Result<Stage, ConfigError> {
        let path = match self.filter {
            FilterSpec::JavaScript(path) if path.is_absolute() => path.into(),
            FilterSpec::JavaScript(path) if path.starts_with(config_dir) => path.into(),
            FilterSpec::JavaScript(path) => config_dir.join(path),
        };
        let filter = JsFilter::new(pipeline.to_owned().into(), index, path)
            .with_config(self.config)
            .with_tick_every_seconds(self.tick_every_seconds);
        let config_topics = topic_filters(self.meta_topics)?;
        Ok(Stage {
            filter,
            config_topics,
        })
    }
}

impl TryFrom<InputConfig> for PipelineInput {
    type Error = ConfigError;

    fn try_from(input: InputConfig) -> Result<Self, Self::Error> {
        match input {
            InputConfig::Mqtt { topics } => Ok(PipelineInput::MQTT {
                topics: topic_filters(topics)?,
            }),
            InputConfig::MeaDB {
                series,
                frequency,
                max_age: span,
            } => {
                let frequency = frequency.as_secs();
                Ok(PipelineInput::MeaDB {
                    series,
                    frequency,
                    max_age: span,
                })
            }
        }
    }
}

fn topic_filters(patterns: Vec<String>) -> Result<TopicFilter, ConfigError> {
    let mut topics = TopicFilter::empty();
    for pattern in patterns {
        topics
            .add(pattern.as_str())
            .map_err(|_| ConfigError::IncorrectTopicFilter(pattern))?;
    }
    Ok(topics)
}

impl Default for OutputConfig {
    fn default() -> Self {
        OutputConfig::Mqtt {
            topics: vec!["#".to_string()],
        }
    }
}

impl TryFrom<OutputConfig> for PipelineOutput {
    type Error = ConfigError;

    fn try_from(value: OutputConfig) -> Result<Self, Self::Error> {
        match value {
            OutputConfig::Mqtt {
                topics: output_topics,
            } => Ok(PipelineOutput::MQTT {
                topics: topic_filters(output_topics)?,
            }),
            OutputConfig::MeaDB { series } => Ok(PipelineOutput::MeaDB { series }),
        }
    }
}

pub fn parse_human_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    humantime::parse_duration(&value).map_err(|_| serde::de::Error::custom("Invalid duration"))
}
