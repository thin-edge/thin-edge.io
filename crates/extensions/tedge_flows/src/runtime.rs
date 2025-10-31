use crate::config::FlowConfig;
use crate::flow::DateTime;
use crate::flow::Flow;
use crate::flow::FlowResult;
use crate::flow::Message;
use crate::flow::SourceTag;
use crate::js_runtime::JsRuntime;
use crate::stats::Counter;
use crate::stats::StatsPublisher;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use std::path::Path;
use tedge_mqtt_ext::TopicFilter;
use tokio::fs::read_dir;
use tokio::fs::read_to_string;
use tokio::time::Instant;
use tracing::error;
use tracing::info;
use tracing::warn;

pub struct MessageProcessor {
    pub config_dir: Utf8PathBuf,
    pub flows: HashMap<String, Flow>,
    pub(super) js_runtime: JsRuntime,
    pub stats: Counter,
}

impl MessageProcessor {
    pub fn flow_id(path: impl AsRef<Path>) -> String {
        format!("{}", path.as_ref().display())
    }

    pub async fn try_new(config_dir: impl AsRef<Utf8Path>) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load(config_dir).await;
        let flows = flow_specs.compile(&mut js_runtime, config_dir).await;
        let stats = Counter::default();

        Ok(MessageProcessor {
            config_dir: config_dir.to_owned(),
            flows,
            js_runtime,
            stats,
        })
    }

    pub async fn try_new_single_flow(
        config_dir: impl AsRef<Utf8Path>,
        flow: impl AsRef<Path>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref();
        let flow = flow.as_ref().to_owned();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load_single_flow(&flow).await;
        let flows = flow_specs.compile(&mut js_runtime, config_dir).await;
        let stats = Counter::default();

        Ok(MessageProcessor {
            config_dir: config_dir.to_owned(),
            flows,
            js_runtime,
            stats,
        })
    }

    pub async fn try_new_single_step_flow(
        config_dir: impl AsRef<Utf8Path>,
        script: impl AsRef<Path>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load_single_script(&script).await;
        let flows = flow_specs.compile(&mut js_runtime, config_dir).await;
        let stats = Counter::default();

        Ok(MessageProcessor {
            config_dir: config_dir.to_owned(),
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

    fn deadlines(&self) -> impl Iterator<Item = tokio::time::Instant> + '_ {
        let script_deadlines = self
            .flows
            .values()
            .flat_map(|flow| &flow.steps)
            .filter_map(|step| step.script.next_execution);

        let source_deadlines = self
            .flows
            .values()
            .filter_map(|flow| flow.input.next_deadline());

        script_deadlines.chain(source_deadlines)
    }

    /// Get the next deadline for interval execution across all scripts
    /// Returns None if no scripts have intervals configured
    pub fn next_interval_deadline(&self) -> Option<tokio::time::Instant> {
        self.deadlines().min()
    }

    /// Get the last deadline for interval execution across all scripts Returns
    /// None if no scripts have intervals configured
    ///
    /// This is intended for `tedge flows test` to ensure it processes all
    /// intervals
    pub fn last_interval_deadline(&self) -> Option<tokio::time::Instant> {
        self.deadlines().max()
    }

    pub async fn on_source_poll(&mut self, timestamp: DateTime, now: Instant) -> Vec<FlowResult> {
        let mut out_messages = vec![];
        for flow in self.flows.values_mut() {
            let messages = match flow.on_source_poll(timestamp, now).await {
                FlowResult::Ok { messages, .. } => messages,
                error => {
                    out_messages.push(error);
                    continue;
                }
            };
            for message in messages {
                let flow_output = flow
                    .on_message(&self.js_runtime, &mut self.stats, timestamp, &message)
                    .await;
                out_messages.push(flow_output);
            }
        }
        out_messages
    }

    pub async fn on_message(
        &mut self,
        timestamp: DateTime,
        source: &SourceTag,
        message: &Message,
    ) -> Vec<FlowResult> {
        let started_at = self.stats.runtime_on_message_start();

        let mut out_messages = vec![];
        for flow in self.flows.values_mut() {
            let config_result = flow.on_config_update(&self.js_runtime, message).await;
            if config_result.is_err() {
                out_messages.push(config_result);
                continue;
            }
            if flow.accept_message(source, message) {
                let flow_output = flow
                    .on_message(&self.js_runtime, &mut self.stats, timestamp, message)
                    .await;
                out_messages.push(flow_output);
            }
        }

        self.stats.runtime_on_message_done(started_at);
        out_messages
    }

    pub async fn on_interval(&mut self, timestamp: DateTime, now: Instant) -> Vec<FlowResult> {
        let mut out_messages = vec![];
        for flow in self.flows.values_mut() {
            let flow_output = flow
                .on_interval(&self.js_runtime, &mut self.stats, timestamp, now)
                .await;
            out_messages.push(flow_output);
        }
        out_messages
    }

    pub async fn dump_processing_stats<P: StatsPublisher>(&self, publisher: &P) -> Vec<P::Record> {
        self.stats.dump_processing_stats(publisher)
    }

    pub async fn dump_memory_stats(&self) {
        self.js_runtime.dump_memory_stats().await;
    }

    pub async fn reload_script(&mut self, path: Utf8PathBuf) {
        for flow in self.flows.values_mut() {
            for step in &mut flow.steps {
                if step.script.path() == path {
                    match self.js_runtime.load_script(&mut step.script).await {
                        Ok(()) => {
                            step.script.init_next_execution();
                            info!(target: "flows", "Reloaded flow script {path}");
                        }
                        Err(e) => {
                            error!(target: "flows", "Failed to reload flow script {path}: {e}");
                            return;
                        }
                    }
                }
            }
        }
    }

    pub async fn remove_script(&mut self, path: Utf8PathBuf) {
        for (flow_id, flow) in self.flows.iter() {
            for step in flow.steps.iter() {
                if step.script.path() == path {
                    warn!(target: "flows", "Removing a script used by a flow {flow_id}: {path}");
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
                error!(target: "flows", "Failed to parse toml for flow {path}: {e}");
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
                error!(target: "flows", "Failed to compile flow {path}: {e}");
                false
            }
        }
    }

    pub async fn add_flow(&mut self, path: Utf8PathBuf) {
        let flow_id = Self::flow_id(&path);
        if self.load_flow(flow_id, path.clone()).await {
            info!(target: "flows", "Loading flow {path}");
        }
    }

    pub async fn remove_flow(&mut self, path: Utf8PathBuf) {
        let flow_id = Self::flow_id(&path);
        self.flows.remove(&flow_id);
        info!(target: "flows", "Removing flow {path}");
    }
}

#[derive(Default)]
struct FlowSpecs {
    flow_specs: HashMap<String, (Utf8PathBuf, FlowConfig)>,
}

impl FlowSpecs {
    pub async fn load(&mut self, config_dir: &Utf8Path) {
        let Ok(mut entries) = read_dir(config_dir).await.map_err(
            |err| error!(target: "flows", "Failed to read flows from {config_dir}: {err}"),
        ) else {
            return;
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
                        if let Err(err) = self.load_flow(path).await {
                            error!(target: "flows", "Failed to load flow: {err}");
                        }
                    }
                }
            }
        }
    }

    pub async fn load_single_flow(&mut self, flow: &Path) {
        let Some(path) = Utf8Path::from_path(flow).map(|p| p.to_path_buf()) else {
            error!(target: "flows", "Skipping non UTF8 path: {}", flow.display());
            return;
        };
        if let Err(err) = self.load_flow(&path).await {
            error!(target: "flows", "Failed to load flow {path}: {err}");
        }
    }

    pub async fn load_single_script(&mut self, script: impl AsRef<Path>) {
        let script = script.as_ref();
        let Some(path) = Utf8Path::from_path(script).map(|p| p.to_path_buf()) else {
            error!(target: "flows", "Skipping non UTF8 path: {}", script.display());
            return;
        };
        let flow_id = MessageProcessor::flow_id(&path);
        let flow = FlowConfig::from_step(path.to_owned());
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
        config_dir: &Utf8Path,
    ) -> HashMap<String, Flow> {
        let mut flows = HashMap::new();
        for (name, (source, specs)) in self.flow_specs.drain() {
            match specs.compile(js_runtime, config_dir, source).await {
                Ok(flow) => {
                    let _ = flows.insert(name, flow);
                }
                Err(err) => {
                    error!(target: "flows", "Failed to compile flow {name}: {err}")
                }
            }
        }
        flows
    }
}
