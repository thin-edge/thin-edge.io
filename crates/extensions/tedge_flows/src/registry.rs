use crate::config::FlowConfig;
use crate::flow::Flow;
use crate::js_runtime::JsRuntime;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use std::path::Path;
use tokio::fs::read_dir;
use tokio::fs::read_to_string;
use tracing::error;
use tracing::info;
use tracing::warn;

pub struct FlowRegistry {
    config_dir: Utf8PathBuf,
    flows: HashMap<String, Flow>,
}

impl FlowRegistry {
    pub fn new(config_dir: impl AsRef<Utf8Path>) -> Self {
        FlowRegistry {
            config_dir: config_dir.as_ref().to_owned(),
            flows: HashMap::new(),
        }
    }

    pub async fn load_all_flows(&mut self, js_runtime: &mut JsRuntime) {
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load(&self.config_dir).await;
        self.flows = flow_specs.compile(js_runtime, &self.config_dir).await
    }

    pub async fn load_single_flow(&mut self, js_runtime: &mut JsRuntime, flow: impl AsRef<Path>) {
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load_single_flow(flow.as_ref()).await;
        self.flows = flow_specs.compile(js_runtime, &self.config_dir).await
    }

    pub async fn load_single_script(
        &mut self,
        js_runtime: &mut JsRuntime,
        script: impl AsRef<Path>,
    ) {
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load_single_script(script.as_ref()).await;
        self.flows = flow_specs.compile(js_runtime, &self.config_dir).await
    }

    pub fn config_dir(&self) -> &Utf8Path {
        &self.config_dir
    }

    pub fn get(&self, name: &str) -> Option<&Flow> {
        self.flows.get(name)
    }

    pub fn flows(&self) -> impl Iterator<Item = &Flow> {
        self.flows.values()
    }

    pub fn flows_mut(&mut self) -> impl Iterator<Item = &mut Flow> {
        self.flows.values_mut()
    }

    pub async fn reload_script(&mut self, js_runtime: &mut JsRuntime, path: Utf8PathBuf) {
        for flow in self.flows.values_mut() {
            for step in &mut flow.steps {
                if step.script.path() == path {
                    match js_runtime.load_script(&mut step.script).await {
                        Ok(()) => {
                            info!(target: "flows", "Reloading flow script {path}");
                            step.script.init_next_execution();
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

    async fn load_flow(
        &mut self,
        js_runtime: &mut JsRuntime,
        flow_id: String,
        path: Utf8PathBuf,
    ) -> bool {
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
            .compile(js_runtime, &self.config_dir, path.clone())
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

    pub fn flow_id(path: impl AsRef<Path>) -> String {
        format!("{}", path.as_ref().display())
    }

    pub async fn add_flow(&mut self, js_runtime: &mut JsRuntime, path: Utf8PathBuf) {
        let flow_id = Self::flow_id(&path);
        if self.load_flow(js_runtime, flow_id, path.clone()).await {
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
        let flow_id = FlowRegistry::flow_id(&path);
        let flow = FlowConfig::from_step(path.to_owned());
        self.flow_specs.insert(flow_id, (path.to_owned(), flow));
    }

    async fn load_flow(&mut self, file: impl AsRef<Utf8Path>) -> Result<(), LoadError> {
        let path = file.as_ref();
        let flow_id = FlowRegistry::flow_id(path);
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
