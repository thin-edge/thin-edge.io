use crate::config::ConfigError;
use crate::config::FlowConfig;
use crate::flow::Flow;
use crate::js_runtime::JsRuntime;
use async_trait::async_trait;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use tracing::error;
use tracing::info;
use tracing::warn;

#[async_trait]
pub trait FlowRegistry {
    type Flow: Send + AsRef<Flow> + AsMut<Flow>;

    fn compile(flow: Flow) -> Result<Self::Flow, ConfigError>;

    fn store(&self) -> &FlowStore<Self::Flow>;
    fn store_mut(&mut self) -> &mut FlowStore<Self::Flow>;

    fn deadlines(&self) -> impl Iterator<Item = tokio::time::Instant> + '_;
}

pub struct BaseFlowRegistry {
    flows: FlowStore<Flow>,
}

impl BaseFlowRegistry {
    pub fn new(config_dir: impl AsRef<Utf8Path>) -> Self {
        BaseFlowRegistry {
            flows: FlowStore::new(config_dir),
        }
    }
}

#[async_trait]
impl FlowRegistry for BaseFlowRegistry {
    type Flow = Flow;

    fn compile(flow: Flow) -> Result<Flow, ConfigError> {
        Ok(flow)
    }

    fn store(&self) -> &FlowStore<Self::Flow> {
        &self.flows
    }

    fn store_mut(&mut self) -> &mut FlowStore<Self::Flow> {
        &mut self.flows
    }

    fn deadlines(&self) -> impl Iterator<Item = tokio::time::Instant> + '_ {
        self.flows()
            .flat_map(|flow| &flow.steps)
            .filter_map(|step| step.next_execution)
    }
}

#[async_trait]
pub trait FlowRegistryExt: FlowRegistry {
    fn config_dir(&self) -> Utf8PathBuf;

    fn contains_flow(&self, flow: &str) -> bool;
    fn flow(&self, name: &str) -> Option<&Self::Flow>;
    fn flow_mut(&mut self, name: &str) -> Option<&mut Self::Flow>;

    fn flows(&self) -> impl Iterator<Item = &Self::Flow>;
    fn flows_mut(&mut self) -> impl Iterator<Item = &mut Self::Flow>;

    async fn load_all_flows(&mut self, js_runtime: &mut JsRuntime);
    async fn load_single_flow(&mut self, js_runtime: &mut JsRuntime, flow: &Utf8Path);
    async fn load_single_script(&mut self, js_runtime: &mut JsRuntime, script: &Utf8Path);

    async fn add_flow(&mut self, js_runtime: &mut JsRuntime, path: &Utf8Path);
    async fn remove_flow(&mut self, path: &Utf8Path);
    async fn reload_script(&mut self, js_runtime: &mut JsRuntime, path: Utf8PathBuf);
    async fn remove_script(&mut self, path: Utf8PathBuf);

    async fn load_config(
        &mut self,
        js_runtime: &mut JsRuntime,
        path: &Utf8Path,
        config: FlowConfig,
    );
}

#[async_trait]
impl<T: FlowRegistry + Send> FlowRegistryExt for T {
    fn config_dir(&self) -> Utf8PathBuf {
        self.store().config_dir.clone()
    }

    fn contains_flow(&self, flow: &str) -> bool {
        self.store().contains_flow(flow)
    }

    fn flow(&self, name: &str) -> Option<&Self::Flow> {
        self.store().flow(name)
    }

    fn flow_mut(&mut self, name: &str) -> Option<&mut Self::Flow> {
        self.store_mut().flow_mut(name)
    }

    fn flows(&self) -> impl Iterator<Item = &Self::Flow> {
        self.store().flows()
    }

    fn flows_mut(&mut self) -> impl Iterator<Item = &mut Self::Flow> {
        self.store_mut().flows_mut()
    }

    async fn load_all_flows(&mut self, js_runtime: &mut JsRuntime) {
        let config_dir = self.config_dir().to_owned();
        for (path, config) in FlowConfig::load_all_flows(&config_dir).await.into_iter() {
            self.load_config(js_runtime, &path, config).await;
        }
    }

    async fn load_single_flow(&mut self, js_runtime: &mut JsRuntime, flow: &Utf8Path) {
        if let Some(config) = FlowConfig::load_single_flow(flow).await {
            self.load_config(js_runtime, flow, config).await;
        }
    }

    async fn load_single_script(&mut self, js_runtime: &mut JsRuntime, script: &Utf8Path) {
        let config = FlowConfig::wrap_script_into_flow(script);
        self.load_config(js_runtime, script, config).await;
    }

    async fn add_flow(&mut self, js_runtime: &mut JsRuntime, path: &Utf8Path) {
        if tokio::fs::read_to_string(&path).await.is_err() {
            self.remove_flow(path).await;
            return;
        };
        info!(target: "flows", "Loading flow {path}");
        if let Some(config) = FlowConfig::load_single_flow(path).await {
            self.load_config(js_runtime, path, config).await;
        }
    }

    async fn remove_flow(&mut self, path: &Utf8Path) {
        self.store_mut().remove(path.as_str());
        info!(target: "flows", "Removing flow {path}");
    }

    async fn reload_script(&mut self, js_runtime: &mut JsRuntime, path: Utf8PathBuf) {
        for flow in self.store_mut().flows_mut() {
            for step in &mut flow.as_mut().steps {
                if step.path() == Some(&path) {
                    match step.load_script(js_runtime).await {
                        Ok(()) => {
                            info!(target: "flows", "Reloading flow script {path}");
                            step.init_next_execution();
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

    async fn remove_script(&mut self, path: Utf8PathBuf) {
        for flow in self.store().flows() {
            let flow_id = flow.as_ref().name();
            for step in flow.as_ref().steps.iter() {
                if step.path() == Some(&path) {
                    warn!(target: "flows", "Removing a script used by a flow {flow_id}: {path}");
                    return;
                }
            }
        }
    }

    async fn load_config(
        &mut self,
        js_runtime: &mut JsRuntime,
        path: &Utf8Path,
        config: FlowConfig,
    ) {
        match config
            .compile(js_runtime, self.store().config_dir(), path.to_owned())
            .await
            .and_then(Self::compile)
        {
            Ok(flow) => {
                self.store_mut().insert(flow);
            }
            Err(err) => {
                error!(target: "flows", "Failed to compile flow {path}: {err}")
            }
        }
    }
}

pub struct FlowStore<F> {
    config_dir: Utf8PathBuf,
    flows: HashMap<String, F>,
}

impl<F> FlowStore<F> {
    pub fn new(config_dir: impl AsRef<Utf8Path>) -> Self {
        FlowStore {
            config_dir: config_dir.as_ref().to_owned(),
            flows: HashMap::new(),
        }
    }

    pub fn config_dir(&self) -> &Utf8Path {
        &self.config_dir
    }

    pub fn contains_flow(&self, flow: &str) -> bool {
        self.flows.contains_key(flow)
    }

    pub fn flow(&self, name: &str) -> Option<&F> {
        self.flows.get(name)
    }

    pub fn flow_mut(&mut self, name: &str) -> Option<&mut F> {
        self.flows.get_mut(name)
    }

    pub fn flows(&self) -> impl Iterator<Item = &F> {
        self.flows.values()
    }

    pub fn flows_mut(&mut self) -> impl Iterator<Item = &mut F> {
        self.flows.values_mut()
    }
}

impl<F: AsRef<Flow>> FlowStore<F> {
    pub fn insert(&mut self, flow: F) {
        self.flows.insert(flow.as_ref().name().to_owned(), flow);
    }

    pub fn remove(&mut self, name: &str) -> Option<F> {
        self.flows.remove(name)
    }
}
