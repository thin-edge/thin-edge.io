use crate::config::PipelineConfig;
use crate::js_runtime::JsRuntime;
use crate::pipeline::DateTime;
use crate::pipeline::FilterError;
use crate::pipeline::Message;
use crate::pipeline::Pipeline;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use tedge_mqtt_ext::TopicFilter;
use tokio::fs::read_dir;
use tokio::fs::read_to_string;
use tracing::error;
use tracing::info;
use tracing::warn;

pub struct MessageProcessor {
    pub config_dir: PathBuf,
    pub pipelines: HashMap<String, Pipeline>,
    pub(super) js_runtime: JsRuntime,
}

impl MessageProcessor {
    pub fn pipeline_id(path: impl AsRef<Path>) -> String {
        format!("{}", path.as_ref().display())
    }

    pub async fn try_new(config_dir: impl AsRef<Path>) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref().to_owned();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut pipeline_specs = PipelineSpecs::default();
        pipeline_specs.load(&mut js_runtime, &config_dir).await;
        let pipelines = pipeline_specs.compile(&js_runtime, &config_dir);

        Ok(MessageProcessor {
            config_dir,
            pipelines,
            js_runtime,
        })
    }

    pub async fn try_new_single_pipeline(
        config_dir: impl AsRef<Path>,
        pipeline: impl AsRef<Path>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref().to_owned();
        let pipeline = pipeline.as_ref().to_owned();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut pipeline_specs = PipelineSpecs::default();
        pipeline_specs
            .load_single_pipeline(&mut js_runtime, &config_dir, &pipeline)
            .await;
        let pipelines = pipeline_specs.compile(&js_runtime, &config_dir);
        Ok(MessageProcessor {
            config_dir,
            pipelines,
            js_runtime,
        })
    }

    pub async fn try_new_single_filter(
        config_dir: impl AsRef<Path>,
        filter: impl AsRef<Path>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref().to_owned();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut pipeline_specs = PipelineSpecs::default();
        pipeline_specs
            .load_single_filter(&mut js_runtime, &filter)
            .await;
        let pipelines = pipeline_specs.compile(&js_runtime, &config_dir);
        Ok(MessageProcessor {
            config_dir,
            pipelines,
            js_runtime,
        })
    }

    pub fn subscriptions(&self) -> TopicFilter {
        let mut topics = TopicFilter::empty();
        for pipeline in self.pipelines.values() {
            topics.add_all(pipeline.topics())
        }
        topics
    }

    pub async fn process(
        &mut self,
        timestamp: &DateTime,
        message: &Message,
    ) -> Vec<(String, Result<Vec<Message>, FilterError>)> {
        let mut out_messages = vec![];
        for (pipeline_id, pipeline) in self.pipelines.iter_mut() {
            let pipeline_output = pipeline.process(&self.js_runtime, timestamp, message).await;
            out_messages.push((pipeline_id.clone(), pipeline_output));
        }
        out_messages
    }

    pub async fn tick(
        &mut self,
        timestamp: &DateTime,
    ) -> Vec<(String, Result<Vec<Message>, FilterError>)> {
        let mut out_messages = vec![];
        for (pipeline_id, pipeline) in self.pipelines.iter_mut() {
            let pipeline_output = pipeline.tick(&self.js_runtime, timestamp).await;
            out_messages.push((pipeline_id.clone(), pipeline_output));
        }
        out_messages
    }

    pub async fn dump_memory_stats(&self) {
        self.js_runtime.dump_memory_stats().await;
    }

    pub async fn add_filter(&mut self, path: Utf8PathBuf) {
        match self.js_runtime.load_file(&path).await {
            Ok(()) => {
                info!(target: "gen-mapper", "Loaded filter {path}");
            }
            Err(e) => {
                error!(target: "gen-mapper", "Failed to load filter {path}: {e}");
            }
        }
    }

    pub async fn reload_filter(&mut self, path: Utf8PathBuf) {
        for pipeline in self.pipelines.values_mut() {
            for stage in &mut pipeline.stages {
                if stage.filter.path() == path {
                    match self.js_runtime.load_file(&path).await {
                        Ok(()) => {
                            info!(target: "gen-mapper", "Reloaded filter {path}");
                        }
                        Err(e) => {
                            error!(target: "gen-mapper", "Failed to reload filter {path}: {e}");
                            return;
                        }
                    }
                }
            }
        }
    }

    pub async fn remove_filter(&mut self, path: Utf8PathBuf) {
        for (pipeline_id, pipeline) in self.pipelines.iter() {
            for stage in pipeline.stages.iter() {
                if stage.filter.path() == path {
                    warn!(target: "gen-mapper", "Removing a filter used by {pipeline_id}: {path}");
                    return;
                }
            }
        }
    }

    pub async fn load_pipeline(&mut self, pipeline_id: String, path: Utf8PathBuf) -> bool {
        let Ok(source) = tokio::fs::read_to_string(&path).await else {
            self.remove_pipeline(path).await;
            return false;
        };
        let config: PipelineConfig = match toml::from_str(&source) {
            Ok(config) => config,
            Err(e) => {
                error!(target: "gen-mapper", "Failed to parse toml for pipeline {path}: {e}");
                return false;
            }
        };
        match config.compile(&self.js_runtime, &self.config_dir, path.clone()) {
            Ok(pipeline) => {
                self.pipelines.insert(pipeline_id, pipeline);
                true
            }
            Err(e) => {
                error!(target: "gen-mapper", "Failed to compile pipeline {path}: {e}");
                false
            }
        }
    }

    pub async fn add_pipeline(&mut self, path: Utf8PathBuf) {
        let pipeline_id = Self::pipeline_id(&path);
        if !self.pipelines.contains_key(&pipeline_id)
            && self.load_pipeline(pipeline_id, path.clone()).await
        {
            info!(target: "gen-mapper", "Loaded new pipeline {path}");
        }
    }

    pub async fn reload_pipeline(&mut self, path: Utf8PathBuf) {
        let pipeline_id = Self::pipeline_id(&path);
        if self.pipelines.contains_key(&pipeline_id)
            && self.load_pipeline(pipeline_id, path.clone()).await
        {
            info!(target: "gen-mapper", "Reloaded updated pipeline {path}");
        }
    }

    pub async fn remove_pipeline(&mut self, path: Utf8PathBuf) {
        let pipeline_id = Self::pipeline_id(&path);
        self.pipelines.remove(&pipeline_id);
        info!(target: "gen-mapper", "Removed deleted pipeline {path}");
    }
}

#[derive(Default)]
struct PipelineSpecs {
    pipeline_specs: HashMap<String, (Utf8PathBuf, PipelineConfig)>,
}

impl PipelineSpecs {
    pub async fn load(&mut self, js_runtime: &mut JsRuntime, config_dir: &PathBuf) {
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
                            if let Err(err) = self.load_filter(js_runtime, path).await {
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
    }

    pub async fn load_single_pipeline(
        &mut self,
        js_runtime: &mut JsRuntime,
        config_dir: &PathBuf,
        pipeline: &Path,
    ) {
        let Some(path) = Utf8Path::from_path(pipeline).map(|p| p.to_path_buf()) else {
            error!(target: "MAPPING", "Skipping non UTF8 path: {}", pipeline.display());
            return;
        };
        if let Err(err) = self.load_pipeline(&path).await {
            error!(target: "MAPPING", "Failed to load pipeline {path}: {err}");
            return;
        }

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
                if file_type.is_file() {
                    match path.extension() {
                        Some("js") | Some("ts") => {
                            info!(target: "MAPPING", "Loading filter: {path}");
                            if let Err(err) = self.load_filter(js_runtime, path).await {
                                error!(target: "MAPPING", "Failed to load filter: {err}");
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    pub async fn load_single_filter(
        &mut self,
        js_runtime: &mut JsRuntime,
        filter: impl AsRef<Path>,
    ) {
        let filter = filter.as_ref();
        let Some(path) = Utf8Path::from_path(filter).map(|p| p.to_path_buf()) else {
            error!(target: "MAPPING", "Skipping non UTF8 path: {}", filter.display());
            return;
        };
        if let Err(err) = js_runtime.load_file(&path).await {
            error!(target: "MAPPING", "Failed to load filter {path}: {err}");
        }
        let pipeline_id = MessageProcessor::pipeline_id(&path);
        let pipeline = PipelineConfig::from_filter(path.to_owned());
        self.pipeline_specs
            .insert(pipeline_id, (path.to_owned(), pipeline));
    }

    async fn load_pipeline(&mut self, file: impl AsRef<Utf8Path>) -> Result<(), LoadError> {
        let path = file.as_ref();
        let pipeline_id = MessageProcessor::pipeline_id(path);
        let specs = read_to_string(path).await?;
        let pipeline: PipelineConfig = toml::from_str(&specs)?;
        self.pipeline_specs
            .insert(pipeline_id, (path.to_owned(), pipeline));

        Ok(())
    }

    async fn load_filter(
        &mut self,
        js_runtime: &mut JsRuntime,
        file: impl AsRef<Path>,
    ) -> Result<(), LoadError> {
        js_runtime.load_file(file).await?;
        Ok(())
    }

    fn compile(mut self, js_runtime: &JsRuntime, config_dir: &Path) -> HashMap<String, Pipeline> {
        let mut pipelines = HashMap::new();
        for (name, (source, specs)) in self.pipeline_specs.drain() {
            match specs.compile(js_runtime, config_dir, source) {
                Ok(pipeline) => {
                    let _ = pipelines.insert(name, pipeline);
                }
                Err(err) => {
                    error!(target: "MAPPING", "Failed to compile pipeline {name}: {err}")
                }
            }
        }
        pipelines
    }
}
