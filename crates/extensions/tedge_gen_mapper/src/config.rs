use crate::pipeline::Pipeline;
use crate::pipeline::Stage;
use crate::gen_filter::GenFilter;
use serde::Deserialize;
use std::path::PathBuf;
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
    JavaScript(PathBuf),
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Not a valid MQTT topic filter: {0}")]
    IncorrectTopicFilter(String),
}

impl TryFrom<PipelineConfig> for Pipeline {
    type Error = ConfigError;

    fn try_from(config: PipelineConfig) -> Result<Self, Self::Error> {
        let input = topic_filters(&config.input_topics)?;
        let stages = config
            .stages
            .into_iter()
            .map(Stage::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Pipeline { input_topics: input, stages })
    }
}

impl TryFrom<StageConfig> for Stage {
    type Error = ConfigError;

    fn try_from(config: StageConfig) -> Result<Self, Self::Error> {
        let filter = match config.filter {
            FilterSpec::JavaScript(path) => GenFilter::new(path),
        };
        let config = topic_filters(&config.config_topics)?;
        Ok(Stage {
            filter: Box::new(filter),
            config_topics: config,
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
