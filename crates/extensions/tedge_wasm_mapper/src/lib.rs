mod actor;
mod config;
mod engine;
mod pipeline;
mod wasm;

use crate::actor::WasmMapper;
use crate::config::PipelineConfig;
use crate::engine::HostEngine;
use crate::pipeline::Pipeline;
use camino::Utf8Path;
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::Path;
use std::path::PathBuf;
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

pub struct WasmMapperBuilder {
    engine: HostEngine,
    message_box: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
    pipeline_specs: HashMap<String, PipelineConfig>,
    pipelines: HashMap<String, Pipeline>,
}

impl WasmMapperBuilder {
    pub fn try_new() -> Result<Self, LoadError> {
        Ok(WasmMapperBuilder {
            engine: HostEngine::try_new()?,
            message_box: SimpleMessageBoxBuilder::new("WasmMapper", 16),
            pipeline_specs: HashMap::default(),
            pipelines: HashMap::default(),
        })
    }

    pub async fn load(&mut self, config_dir: impl AsRef<Path>) {
        let config_dir = config_dir.as_ref();
        let Ok(mut entries) = read_dir(config_dir).await.map_err(|err|
            error!(target: "WASM", "Failed to read filters from {}: {err}", config_dir.display())
        ) else {
            return;
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let Some(path) = Utf8Path::from_path(&entry.path()).map(|p| p.to_path_buf()) else {
                error!(target: "WASM", "Skipping non UTF8 path: {}", entry.path().display());
                continue;
            };
            if let Ok(file_type) = entry.file_type().await {
                if file_type.is_file() {
                    match path.extension() {
                        Some("toml") => {
                            info!(target: "WASM", "Loading pipeline: {path}");
                            if let Err(err) = self.load_pipeline(path).await {
                                error!(target: "WASM", "Failed to load pipeline: {err}");
                            }
                        }
                        Some("wasm") => {
                            info!(target: "WASM", "Loading filter: {path}");
                            if let Err(err) = self.engine.load_component(path).await {
                                error!(target: "WASM", "Failed to load filter: {err}");
                            }
                        }
                        _ => {
                            info!(target: "WASM", "Skipping file which type is unknown: {path}");
                        }
                    }
                }
            }
        }

        // FIXME This should be done when the actor is built
        // Done here for now to ease the computation of the topics to subscribe to
        // as these topics have to be known when connect is called
        self.instantiate()
    }

    async fn load_pipeline(&mut self, file: impl AsRef<Utf8Path>) -> Result<(), LoadError> {
        if let Some(name) = file.as_ref().file_name() {
            let specs = read_to_string(file.as_ref()).await?;
            let pipeline: PipelineConfig = toml::from_str(&specs)?;
            self.pipeline_specs.insert(name.to_string(), pipeline);
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

    fn instantiate(&mut self) {
        for (name, specs) in self.pipeline_specs.drain() {
            match specs.instantiate(&self.engine) {
                Ok(pipeline) => {
                    let _ = self.pipelines.insert(name, pipeline);
                }
                Err(err) => {
                    error!(target: "WASM", "Failed to load pipeline: {err}")
                }
            }
        }
    }

    fn topics(&self) -> TopicFilter {
        let mut topics = TopicFilter::empty();
        for pipeline in self.pipelines.values() {
            topics.add_all(pipeline.topics())
        }
        topics
    }
}

impl RuntimeRequestSink for WasmMapperBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}

impl Builder<WasmMapper> for WasmMapperBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<WasmMapper, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> WasmMapper {
        WasmMapper {
            mqtt: self.message_box.build(),
            pipelines: self.pipelines,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum LoadError {
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    TomlError(#[from] toml::de::Error),

    #[error(transparent)]
    WasmError(#[from] wasmtime::Error),

    #[error("Failed to import {import}: {error}")]
    WasmFailedImport {
        import: String,
        error: wasmtime::Error,
    },

    #[error(transparent)]
    ConfigError(#[from] config::ConfigError),
}
