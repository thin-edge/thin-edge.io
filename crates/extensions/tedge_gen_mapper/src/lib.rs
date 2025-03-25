mod actor;
mod config;
mod pipeline;
mod gen_filter;

use crate::actor::GenMapper;
use crate::pipeline::Pipeline;
use camino::Utf8Path;
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::Path;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tokio::fs::read_dir;
use tokio::fs::read_to_string;
use tracing::error;
use tracing::info;

pub struct GenMapperBuilder {
    message_box: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
    pipelines: HashMap<String, Pipeline>,
}

impl Default for GenMapperBuilder {
    fn default() -> Self {
        GenMapperBuilder {
            message_box: SimpleMessageBoxBuilder::new("GenMapper", 16),
            pipelines: HashMap::default(),
        }
    }
}

impl GenMapperBuilder {
    pub async fn load(&mut self, config_dir: impl AsRef<Path>) {
        let config_dir = config_dir.as_ref();
        let Ok(mut entries) = read_dir(config_dir).await.map_err(|err|
            error!(target: "MAPPING", "Failed to read filters from {}: {err}", config_dir.display())
        ) else {
            return;
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let Some(path) = Utf8Path::from_path(&entry.path()).map(|p| p.to_path_buf()) else {
                error!(target: "MAPPING", "Skipping non UTF8 path: {}", entry.path().display());
                continue;
            };
            if let Ok(file_type) = entry.file_type().await {
                if file_type.is_file() && path.extension() == Some("toml") {
                    info!(target: "MAPPING", "Loading pipeline: {path}");
                    if let Err(err) = self.load_pipeline(path).await {
                        error!(target: "MAPPING", "Failed to load pipeline: {err}");
                    }
                }
            }
        }
    }

    async fn load_pipeline(&mut self, file: impl AsRef<Utf8Path>) -> Result<(), LoadError> {
        if let Some(name) = file.as_ref().file_name() {
            let specs = read_to_string(file.as_ref()).await?;
            let pipeline: Pipeline = toml::from_str(&specs)?;
            self.pipelines.insert(name.to_string(), pipeline);
        }

        Ok(())
    }

    pub fn connect(
        &mut self,
        mqtt: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
    ) {
        mqtt.connect_sink(self.topics(), &self.message_box);
        self.message_box.connect_sink(NoConfig, mqtt);
    }

    fn topics(&self) -> TopicFilter {
        let mut topics = TopicFilter::empty();
        for pipeline in self.pipelines.values() {
            topics.add_all(pipeline.topics())
        }
        topics
    }
}

impl RuntimeRequestSink for GenMapperBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}

impl Builder<GenMapper> for GenMapperBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<GenMapper, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> GenMapper {
        GenMapper {
            mqtt: self.message_box.build(),
            pipelines: self.pipelines,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum LoadError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    TomlError(#[from] toml::de::Error),
}
