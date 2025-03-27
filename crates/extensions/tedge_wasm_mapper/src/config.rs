use crate::engine::HostEngine;
use crate::pipeline::Pipeline;
use crate::pipeline::Stage;
use crate::LoadError;
use camino::Utf8PathBuf;
use serde::Deserialize;
use tedge_mqtt_ext::TopicFilter;

#[derive(Deserialize)]
pub struct PipelineConfig {
    input_topics: Vec<String>,
    stages: Vec<StageConfig>,
}

#[derive(Deserialize)]
pub struct StageConfig {
    filter: FilterSpec,

    #[serde(default)]
    config_topics: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum FilterSpec {
    Wasm(Utf8PathBuf),
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Not a valid MQTT topic filter: {0}")]
    IncorrectTopicFilter(String),
}

impl PipelineConfig {
    pub fn instantiate(self, engine: &HostEngine) -> Result<Pipeline, LoadError> {
        let input = topic_filters(&self.input_topics)?;
        let stages = self
            .stages
            .into_iter()
            .map(|stage| stage.instantiate(engine))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Pipeline {
            input_topics: input,
            stages,
        })
    }
}

impl StageConfig {
    fn instantiate(self, engine: &HostEngine) -> Result<Stage, LoadError> {
        let filter = match self.filter {
            FilterSpec::Wasm(path) => engine.instantiate(path.as_path())?,
        };
        let config_topics = topic_filters(&self.config_topics)?;
        Ok(Stage {
            filter: Box::new(filter),
            config_topics,
        })
    }
}

fn topic_filters(patterns: &Vec<String>) -> Result<TopicFilter, ConfigError> {
    let mut topics = TopicFilter::empty();
    for pattern in patterns {
        topics
            .add(pattern.as_str())
            .map_err(|_| ConfigError::IncorrectTopicFilter(pattern.clone()))?;
    }
    Ok(topics)
}
