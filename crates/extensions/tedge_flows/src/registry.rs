use crate::config::ConfigError;
use crate::config::FlowConfig;
use crate::flow::Flow;
use crate::js_runtime::JsRuntime;
use crate::transformers::BuiltinTransformers;
use crate::transformers::Transformer;
use crate::transformers::TransformerBuilder;
use async_trait::async_trait;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use std::collections::HashSet;
use tedge_utils::file;
use tedge_utils::fs;
use tracing::error;
use tracing::info;
use tracing::warn;

#[async_trait]
pub trait FlowRegistry {
    type Flow: Send + AsRef<Flow> + AsMut<Flow>;

    fn compile(flow: Flow) -> Result<Self::Flow, ConfigError>;

    fn builtins(&self) -> &BuiltinTransformers;
    fn builtins_mut(&mut self) -> &mut BuiltinTransformers;

    fn store(&self) -> &FlowStore<Self::Flow>;
    fn store_mut(&mut self) -> &mut FlowStore<Self::Flow>;

    fn deadlines(&self) -> impl Iterator<Item = tokio::time::Instant> + '_;
}

pub struct BaseFlowRegistry {
    flows: FlowStore<Flow>,
    builtins: BuiltinTransformers,
}

impl BaseFlowRegistry {
    pub fn new(config_dir: impl AsRef<Utf8Path>) -> Self {
        BaseFlowRegistry {
            flows: FlowStore::new(config_dir),
            builtins: BuiltinTransformers::default(),
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

    fn builtins(&self) -> &BuiltinTransformers {
        &self.builtins
    }

    fn builtins_mut(&mut self) -> &mut BuiltinTransformers {
        &mut self.builtins
    }

    fn deadlines(&self) -> impl Iterator<Item = tokio::time::Instant> + '_ {
        self.flows()
            .flat_map(|flow| &flow.steps)
            .filter_map(|step| step.next_execution)
    }
}

pub enum RegistrationStatus {
    Unregistered,
    Registered,
    Broken,
}

#[async_trait]
pub trait FlowRegistryExt: FlowRegistry {
    fn config_dir(&self) -> Utf8PathBuf;

    fn registration_status(&self, path: &Utf8Path) -> RegistrationStatus;
    fn flow(&self, path: &Utf8Path) -> Option<&Self::Flow>;
    fn flow_mut(&mut self, path: &Utf8Path) -> Option<&mut Self::Flow>;

    fn flows(&self) -> impl Iterator<Item = &Self::Flow>;
    fn flows_mut(&mut self) -> impl Iterator<Item = &mut Self::Flow>;

    async fn load_all_flows(&mut self, js_runtime: &mut JsRuntime);
    async fn load_single_flow(&mut self, js_runtime: &mut JsRuntime, flow: &Utf8Path);
    async fn load_single_script(&mut self, js_runtime: &mut JsRuntime, script: &Utf8Path);

    async fn add_flow(&mut self, js_runtime: &mut JsRuntime, path: &Utf8Path);
    async fn remove_flow(&mut self, path: &Utf8Path);
    async fn reload_script(
        &mut self,
        js_runtime: &mut JsRuntime,
        path: &Utf8Path,
    ) -> Vec<Utf8PathBuf>;
    async fn remove_script(&mut self, path: &Utf8Path);

    async fn load_config(
        &mut self,
        js_runtime: &mut JsRuntime,
        path: &Utf8Path,
        config: FlowConfig,
    );

    /// Create a builtin flow definition in the flow configuration directory.
    ///
    /// This flow definition is persisted as a file
    /// with the given name, a `.toml` extension and the given content.
    ///
    /// A copy of the flow is also persisted with the name and a `.toml.template` extension.
    /// This template can be used by used to derive and tune custom flows.
    /// This template is also used by `tedge flows` as a witness for user updates:
    /// if a flow definition differs with its template, then the flow as updated by the user is kept unchanged.
    ///
    /// Also, if a file exists with the same name and a `.toml.disabled` extension,
    /// then the file for the builtin flow is not created: this is how a user can disable a builtin flow.
    async fn persist_builtin_flow(
        &mut self,
        name: &str,
        content: &str,
    ) -> Result<(), UpdateFlowRegistryError>;

    /// Register a transformer that can be used as a builtin in flow steps
    fn register_builtin(&mut self, transformer: impl TransformerBuilder + Transformer);
}

#[async_trait]
impl<T: FlowRegistry + Send> FlowRegistryExt for T {
    fn config_dir(&self) -> Utf8PathBuf {
        self.store().config_dir.clone()
    }

    fn registration_status(&self, path: &Utf8Path) -> RegistrationStatus {
        self.store().contains_flow(path)
    }

    fn flow(&self, path: &Utf8Path) -> Option<&Self::Flow> {
        self.store().flow(path)
    }

    fn flow_mut(&mut self, path: &Utf8Path) -> Option<&mut Self::Flow> {
        self.store_mut().flow_mut(path)
    }

    fn flows(&self) -> impl Iterator<Item = &Self::Flow> {
        self.store().flows()
    }

    fn flows_mut(&mut self) -> impl Iterator<Item = &mut Self::Flow> {
        self.store_mut().flows_mut()
    }

    async fn load_all_flows(&mut self, js_runtime: &mut JsRuntime) {
        let config_dir = self.config_dir().to_owned();
        let (loaded_flows, unloaded_flows) = FlowConfig::load_all_flows(&config_dir).await;
        for (path, config) in loaded_flows.into_iter() {
            self.load_config(js_runtime, &path, config).await;
        }
        for unloaded_flow in unloaded_flows.into_iter() {
            self.store_mut().add_unloaded(unloaded_flow);
        }
    }

    async fn load_single_flow(&mut self, js_runtime: &mut JsRuntime, flow: &Utf8Path) {
        if let Some(config) = FlowConfig::load_single_flow(flow).await {
            self.load_config(js_runtime, flow, config).await;
        } else {
            self.store_mut().add_unloaded(flow.to_owned());
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
        } else {
            self.store_mut().add_unloaded(path.to_owned());
        }
    }

    async fn remove_flow(&mut self, path: &Utf8Path) {
        info!(target: "flows", "Removing flow {path}");
        self.store_mut().remove(path);
    }

    async fn reload_script(
        &mut self,
        js_runtime: &mut JsRuntime,
        path: &Utf8Path,
    ) -> Vec<Utf8PathBuf> {
        let mut reloaded_flows = HashSet::new();
        for flow in self.store_mut().flows_mut() {
            let mut reloaded = false;
            for step in &mut flow.as_mut().steps {
                if step.path() == Some(path) {
                    match step.load_script(js_runtime).await {
                        Ok(()) => {
                            reloaded = true;
                            info!(target: "flows", "Reloading flow script {path}");
                        }
                        Err(e) => {
                            error!(target: "flows", "Failed to reload flow script {path}: {e}");
                        }
                    }
                }
            }
            if reloaded {
                reloaded_flows.insert(flow.as_ref().source.clone());
            }
        }

        // reload unloaded flows: they might be fixed by the new script
        let unloaded_flows = self.store_mut().drain_unloaded();
        for path in unloaded_flows {
            self.add_flow(js_runtime, &path).await;
            if self.store().flow(&path).is_some() {
                reloaded_flows.insert(path);
            }
        }

        reloaded_flows.into_iter().collect()
    }

    async fn remove_script(&mut self, path: &Utf8Path) {
        for flow in self.store().flows() {
            let flow_id = flow.as_ref().name();
            for step in flow.as_ref().steps.iter() {
                if step.path() == Some(path) {
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
            .compile(self.builtins(), js_runtime, path.to_owned())
            .await
            .and_then(Self::compile)
        {
            Ok(flow) => {
                self.store_mut().insert(flow);
            }
            Err(err) => {
                error!(target: "flows", "Failed to compile flow {path}: {err}");
                self.store_mut().add_unloaded(path.to_owned());
            }
        }
    }

    async fn persist_builtin_flow(
        &mut self,
        name: &str,
        content: &str,
    ) -> Result<(), UpdateFlowRegistryError> {
        let dir = self.store().config_dir();
        let flow_path = dir.join(name).with_extension("toml");
        let disabled_flow_path = flow_path.with_extension("toml.disabled");
        let template_path = flow_path.with_extension("toml.template");

        // Don't update the flow definition if overridden or disabled
        let prior_flow = tokio::fs::read(&flow_path).await.ok();
        let prior_template = tokio::fs::read(&template_path).await.ok();
        let overridden = prior_flow != prior_template;
        let disabled = tokio::fs::try_exists(&disabled_flow_path)
            .await
            .unwrap_or(false);
        let update_flow = !overridden && !disabled;

        // Persist a copy of flow definition to be used by users as a template for their flows.
        file::create_directory_with_defaults(dir).await?;
        fs::atomically_write_file_async(template_path.as_std_path(), content.as_bytes()).await?;

        if update_flow {
            fs::atomically_write_file_async(flow_path.as_std_path(), content.as_bytes()).await?;
        }

        Ok(())
    }

    fn register_builtin(&mut self, transformer: impl TransformerBuilder + Transformer) {
        self.builtins_mut().register(transformer)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum UpdateFlowRegistryError {
    #[error(transparent)]
    FileError(#[from] file::FileError),

    #[error(transparent)]
    FileMoveError(#[from] file::FileMoveError),

    #[error(transparent)]
    AtomicFileError(#[from] fs::AtomFileError),
}

pub struct FlowStore<F> {
    config_dir: Utf8PathBuf,
    flows: HashMap<Utf8PathBuf, F>,
    unloaded_flows: HashSet<Utf8PathBuf>,
}

impl<F> FlowStore<F> {
    pub fn new(config_dir: impl AsRef<Utf8Path>) -> Self {
        FlowStore {
            config_dir: config_dir.as_ref().to_owned(),
            flows: HashMap::new(),
            unloaded_flows: HashSet::new(),
        }
    }

    pub fn config_dir(&self) -> &Utf8Path {
        &self.config_dir
    }

    pub fn contains_flow(&self, flow: &Utf8Path) -> RegistrationStatus {
        if self.unloaded_flows.contains(flow) {
            RegistrationStatus::Broken
        } else if self.flows.contains_key(flow) {
            RegistrationStatus::Registered
        } else {
            RegistrationStatus::Unregistered
        }
    }

    pub fn flow(&self, name: &Utf8Path) -> Option<&F> {
        self.flows.get(name)
    }

    pub fn flow_mut(&mut self, name: &Utf8Path) -> Option<&mut F> {
        self.flows.get_mut(name)
    }

    pub fn flows(&self) -> impl Iterator<Item = &F> {
        self.flows.values()
    }

    pub fn flows_mut(&mut self) -> impl Iterator<Item = &mut F> {
        self.flows.values_mut()
    }

    pub fn add_unloaded(&mut self, path: Utf8PathBuf) {
        self.unloaded_flows.insert(path);
    }

    pub fn drain_unloaded(&mut self) -> Vec<Utf8PathBuf> {
        self.unloaded_flows.drain().collect()
    }
}

impl<F: AsRef<Flow>> FlowStore<F> {
    pub fn insert(&mut self, flow: F) {
        self.unloaded_flows.remove(&flow.as_ref().source);
        self.flows.insert(flow.as_ref().source.to_owned(), flow);
    }

    pub fn remove(&mut self, flow: &Utf8Path) -> Option<F> {
        self.unloaded_flows.remove(flow);
        self.flows.remove(flow)
    }
}
