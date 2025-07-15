use crate::config::FlowConfig;
use crate::flow::DateTime;
use crate::flow::FilterError;
use crate::flow::Flow;
use crate::flow::Message;
use crate::js_runtime::JsRuntime;
use crate::stats::Counter;
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
    pub flows: HashMap<String, Flow>,
    pub(super) js_runtime: JsRuntime,
    pub stats: Counter,
}

impl MessageProcessor {
    pub fn flow_id(path: impl AsRef<Path>) -> String {
        format!("{}", path.as_ref().display())
    }

    pub async fn try_new(config_dir: impl AsRef<Path>) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref().to_owned();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load(&config_dir).await;
        let flows = flow_specs.compile(&mut js_runtime, &config_dir).await;
        let stats = Counter::default();

        Ok(MessageProcessor {
            config_dir,
            flows,
            js_runtime,
            stats,
        })
    }

    pub async fn try_new_single_flow(
        config_dir: impl AsRef<Path>,
        flow: impl AsRef<Path>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref().to_owned();
        let flow = flow.as_ref().to_owned();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load_single_flow(&flow).await;
        let flows = flow_specs.compile(&mut js_runtime, &config_dir).await;
        let stats = Counter::default();

        Ok(MessageProcessor {
            config_dir,
            flows,
            js_runtime,
            stats,
        })
    }

    pub async fn try_new_single_filter(
        config_dir: impl AsRef<Path>,
        filter: impl AsRef<Path>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref().to_owned();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load_single_filter(&filter).await;
        let flows = flow_specs.compile(&mut js_runtime, &config_dir).await;
        let stats = Counter::default();

        Ok(MessageProcessor {
            config_dir,
            flows,
            js_runtime,
            stats,
        })
    }

    pub fn subscriptions(&self) -> TopicFilter {
        let mut topics = TopicFilter::empty();
        for flow in self.flows.values() {
            topics.add_all(flow.topics())
        }
        topics
    }

    pub async fn process(
        &mut self,
        timestamp: &DateTime,
        message: &Message,
    ) -> Vec<(String, Result<Vec<Message>, FilterError>)> {
        let started_at = self.stats.runtime_process_start();

        let mut out_messages = vec![];
        for (flow_id, flow) in self.flows.iter_mut() {
            let flow_output = flow
                .process(&self.js_runtime, &mut self.stats, timestamp, message)
                .await;
            if flow_output.is_err() {
                self.stats.flow_process_failed(flow_id);
            }
            out_messages.push((flow_id.clone(), flow_output));
        }

        self.stats.runtime_process_done(started_at);
        out_messages
    }

    pub async fn tick(
        &mut self,
        timestamp: &DateTime,
    ) -> Vec<(String, Result<Vec<Message>, FilterError>)> {
        let mut out_messages = vec![];
        for (flow_id, flow) in self.flows.iter_mut() {
            let flow_output = flow
                .tick(&self.js_runtime, &mut self.stats, timestamp)
                .await;
            if flow_output.is_err() {
                self.stats.flow_tick_failed(flow_id);
            }
            out_messages.push((flow_id.clone(), flow_output));
        }
        out_messages
    }

    pub async fn dump_processing_stats(&self) {
        self.stats.dump_processing_stats();
    }

    pub async fn dump_memory_stats(&self) {
        self.js_runtime.dump_memory_stats().await;
    }

    pub async fn reload_filter(&mut self, path: Utf8PathBuf) {
        for flow in self.flows.values_mut() {
            for stage in &mut flow.stages {
                if stage.filter.path() == path {
                    match self.js_runtime.load_filter(&mut stage.filter).await {
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
        for (flow_id, flow) in self.flows.iter() {
            for stage in flow.stages.iter() {
                if stage.filter.path() == path {
                    warn!(target: "gen-mapper", "Removing a filter used by {flow_id}: {path}");
                    return;
                }
            }
        }
    }

    pub async fn load_flow(&mut self, flow_id: String, path: Utf8PathBuf) -> bool {
        let Ok(source) = tokio::fs::read_to_string(&path).await else {
            self.remove_flow(path).await;
            return false;
        };
        let config: FlowConfig = match toml::from_str(&source) {
            Ok(config) => config,
            Err(e) => {
                error!(target: "gen-mapper", "Failed to parse toml for flow {path}: {e}");
                return false;
            }
        };
        match config
            .compile(&mut self.js_runtime, &self.config_dir, path.clone())
            .await
        {
            Ok(flow) => {
                self.flows.insert(flow_id, flow);
                true
            }
            Err(e) => {
                error!(target: "gen-mapper", "Failed to compile flow {path}: {e}");
                false
            }
        }
    }

    pub async fn add_flow(&mut self, path: Utf8PathBuf) {
        let flow_id = Self::flow_id(&path);
        if !self.flows.contains_key(&flow_id) && self.load_flow(flow_id, path.clone()).await {
            info!(target: "gen-mapper", "Loaded new flow {path}");
        }
    }

    pub async fn reload_flow(&mut self, path: Utf8PathBuf) {
        let flow_id = Self::flow_id(&path);
        if self.flows.contains_key(&flow_id) && self.load_flow(flow_id, path.clone()).await {
            info!(target: "gen-mapper", "Reloaded updated flow {path}");
        }
    }

    pub async fn remove_flow(&mut self, path: Utf8PathBuf) {
        let flow_id = Self::flow_id(&path);
        self.flows.remove(&flow_id);
        info!(target: "gen-mapper", "Removed deleted flow {path}");
    }
}

#[derive(Default)]
struct FlowSpecs {
    flow_specs: HashMap<String, (Utf8PathBuf, FlowConfig)>,
}

impl FlowSpecs {
    pub async fn load(&mut self, config_dir: &PathBuf) {
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
                    if let Some("toml") = path.extension() {
                        info!(target: "MAPPING", "Loading flow: {path}");
                        if let Err(err) = self.load_flow(path).await {
                            error!(target: "MAPPING", "Failed to load flow: {err}");
                        }
                    }
                }
            }
        }
    }

    pub async fn load_single_flow(&mut self, flow: &Path) {
        let Some(path) = Utf8Path::from_path(flow).map(|p| p.to_path_buf()) else {
            error!(target: "MAPPING", "Skipping non UTF8 path: {}", flow.display());
            return;
        };
        if let Err(err) = self.load_flow(&path).await {
            error!(target: "MAPPING", "Failed to load flow {path}: {err}");
        }
    }

    pub async fn load_single_filter(&mut self, filter: impl AsRef<Path>) {
        let filter = filter.as_ref();
        let Some(path) = Utf8Path::from_path(filter).map(|p| p.to_path_buf()) else {
            error!(target: "MAPPING", "Skipping non UTF8 path: {}", filter.display());
            return;
        };
        let flow_id = MessageProcessor::flow_id(&path);
        let flow = FlowConfig::from_filter(path.to_owned());
        self.flow_specs.insert(flow_id, (path.to_owned(), flow));
    }

    async fn load_flow(&mut self, file: impl AsRef<Utf8Path>) -> Result<(), LoadError> {
        let path = file.as_ref();
        let flow_id = MessageProcessor::flow_id(path);
        let specs = read_to_string(path).await?;
        let flow: FlowConfig = toml::from_str(&specs)?;
        self.flow_specs.insert(flow_id, (path.to_owned(), flow));

        Ok(())
    }

    async fn compile(
        mut self,
        js_runtime: &mut JsRuntime,
        config_dir: &Path,
    ) -> HashMap<String, Flow> {
        let mut flows = HashMap::new();
        for (name, (source, specs)) in self.flow_specs.drain() {
            match specs.compile(js_runtime, config_dir, source).await {
                Ok(flow) => {
                    let _ = flows.insert(name, flow);
                }
                Err(err) => {
                    error!(target: "MAPPING", "Failed to compile flow {name}: {err}")
                }
            }
        }
        flows
    }
}
