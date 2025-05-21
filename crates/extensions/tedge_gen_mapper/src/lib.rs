mod actor;
mod config;
mod js_filter;
mod pipeline;

use crate::actor::GenMapper;
use crate::config::PipelineConfig;
use crate::js_filter::JsRuntime;
use crate::pipeline::Pipeline;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::fan_in_message_type;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tokio::fs::read_dir;
use tokio::fs::read_to_string;
use tracing::error;
use tracing::info;

fan_in_message_type!(InputMessage[MqttMessage, FsWatchEvent]: Clone, Debug, Eq, PartialEq);

pub struct GenMapperBuilder {
    config_dir: PathBuf,
    message_box: SimpleMessageBoxBuilder<InputMessage, MqttMessage>,
    pipelines: HashMap<String, Pipeline>,
    pipeline_specs: HashMap<String, (Utf8PathBuf, PipelineConfig)>,
    js_runtime: JsRuntime,
}

impl GenMapperBuilder {
    pub fn try_new(config_dir: impl AsRef<Path>) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref().to_owned();
        let js_runtime = JsRuntime::try_new()?;
        Ok(GenMapperBuilder {
            config_dir,
            message_box: SimpleMessageBoxBuilder::new("GenMapper", 16),
            pipelines: HashMap::default(),
            pipeline_specs: HashMap::default(),
            js_runtime,
        })
    }

    pub async fn load(&mut self) {
        let Ok(mut entries) = read_dir(&self.config_dir).await.map_err(|err|
            error!(target: "MAPPING", "Failed to read filters from {}: {err}", self.config_dir.display())
        ) else {
            return;
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let Some(path) = Utf8Path::from_path(&entry.path()).map(|p| p.to_path_buf()) else {
                error!(target: "MAPPING", "Skipping non UTF8 path: {}", entry.path().display());
                continue;
            };
            if let Ok(file_type) = entry.file_type().await {
                if file_type.is_file() {
                    match path.extension() {
                        Some("toml") => {
                            info!(target: "MAPPING", "Loading pipeline: {path}");
                            if let Err(err) = self.load_pipeline(path).await {
                                error!(target: "MAPPING", "Failed to load pipeline: {err}");
                            }
                        }
                        Some("js") | Some("ts") => {
                            info!(target: "MAPPING", "Loading filter: {path}");
                            if let Err(err) = self.load_filter(path).await {
                                error!(target: "MAPPING", "Failed to load filter: {err}");
                            }
                        }
                        _ => {
                            info!(target: "MAPPING", "Skipping file which type is unknown: {path}");
                        }
                    }
                }
            }
        }

        // Done here to ease the computation of the topics to subscribe to
        // as these topics have to be known when connect is called
        self.compile()
    }

    async fn load_pipeline(&mut self, file: impl AsRef<Utf8Path>) -> Result<(), LoadError> {
        if let Some(name) = file.as_ref().file_name() {
            let specs = read_to_string(file.as_ref()).await?;
            let pipeline: PipelineConfig = toml::from_str(&specs)?;
            self.pipeline_specs
                .insert(name.to_string(), (file.as_ref().to_owned(), pipeline));
        }

        Ok(())
    }

    async fn load_filter(&mut self, file: impl AsRef<Utf8Path>) -> Result<(), LoadError> {
        self.js_runtime.load_file(file.as_ref())?;
        Ok(())
    }

    fn compile(&mut self) {
        for (name, (source, specs)) in self.pipeline_specs.drain() {
            match specs.compile(&self.js_runtime, &self.config_dir, source) {
                Ok(pipeline) => {
                    let _ = self.pipelines.insert(name, pipeline);
                }
                Err(err) => {
                    error!(target: "MAPPING", "Failed to compile pipeline {name}: {err}")
                }
            }
        }
    }

    pub fn connect(
        &mut self,
        mqtt: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
    ) {
        mqtt.connect_mapped_sink(self.topics(), &self.message_box, |msg| {
            Some(InputMessage::MqttMessage(msg))
        });
        self.message_box.connect_sink(NoConfig, mqtt);
    }

    pub fn connect_fs(&mut self, fs: &mut impl MessageSource<FsWatchEvent, PathBuf>) {
        fs.connect_mapped_sink(self.config_dir.clone(), &self.message_box, |msg| {
            Some(InputMessage::FsWatchEvent(msg))
        });
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
            messages: self.message_box.build(),
            pipelines: self.pipelines,
            js_runtime: self.js_runtime,
            config_dir: self.config_dir,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum LoadError {
    #[error("Script not loaded: {path}")]
    ScriptNotLoaded { path: PathBuf },

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    TomlError(#[from] toml::de::Error),

    #[error(transparent)]
    JsError(#[from] rustyscript::Error),
}
