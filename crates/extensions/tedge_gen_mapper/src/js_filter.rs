use crate::pipeline;
use crate::pipeline::DateTime;
use crate::pipeline::FilterError;
use crate::pipeline::Message;
use crate::LoadError;
use rustyscript::deno_core::ModuleId;
use rustyscript::serde_json::json;
use rustyscript::serde_json::Value;
use rustyscript::worker::DefaultWorker;
use rustyscript::worker::DefaultWorkerOptions;
use rustyscript::Module;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tracing::debug;

#[derive(Clone)]
pub struct JsFilter {
    path: PathBuf,
    module_id: ModuleId,
    config: Value,
    tick_every_seconds: u64,
}

impl JsFilter {
    pub fn new(path: PathBuf, module_id: ModuleId) -> Self {
        JsFilter {
            path,
            module_id,
            config: json!({}),
            tick_every_seconds: 0,
        }
    }

    pub fn with_config(self, config: Option<Value>) -> Self {
        if let Some(config) = config {
            Self { config, ..self }
        } else {
            self
        }
    }

    pub fn with_tick_every_seconds(self, tick_every_seconds: u64) -> Self {
        Self {
            tick_every_seconds,
            ..self
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Process a message returning zero, one or more messages
    ///
    /// The "process" function of the JS module is passed 3 arguments
    /// - the current timestamp
    /// - the message to be transformed
    /// - the filter config (as configured for the pipeline stage, possibly updated by update_config messages)
    ///
    /// The returned value is expected to be an array of messages.
    pub fn process(
        &self,
        js: &JsRuntime,
        timestamp: &DateTime,
        message: &Message,
    ) -> Result<Vec<Message>, FilterError> {
        debug!(target: "MAPPING", "{}: process({timestamp:?}, {message:?})", self.path.display());
        let input = vec![timestamp.json(), message.json(), self.config.clone()];
        js.runtime
            .call_function(Some(self.module_id), "process".to_string(), input)
            .map_err(pipeline::error_from_js)
    }

    /// Update the filter config using a metadata message
    ///
    /// The "update_config" function of the JS module is passed 2 arguments
    /// - the message
    /// - the current filter config
    ///
    /// The value returned by this function is used as the updated filter config
    pub fn update_config(&mut self, js: &JsRuntime, message: &Message) -> Result<(), FilterError> {
        debug!(target: "MAPPING", "{}: update_config({message:?})", self.path.display());
        let input = vec![message.json(), self.config.clone()];
        let config = js
            .runtime
            .call_function(Some(self.module_id), "update_config".to_string(), input)
            .map_err(pipeline::error_from_js)?;
        self.config = config;
        Ok(())
    }

    /// Trigger the tick function of the JS module
    ///
    /// The "tick" function is passed 2 arguments
    /// - the current timestamp
    /// - the current filter config
    ///
    /// Return zero, one or more messages
    pub fn tick(&self, js: &JsRuntime, timestamp: &DateTime) -> Result<Vec<Message>, FilterError> {
        if !timestamp.tick_now(self.tick_every_seconds) {
            return Ok(vec![]);
        }
        debug!(target: "MAPPING", "{}: tick({timestamp:?})", self.path.display());
        let input = vec![timestamp.json(), self.config.clone()];
        js.runtime
            .call_function(Some(self.module_id), "tick".to_string(), input)
            .map_err(pipeline::error_from_js)
    }
}

pub struct JsRuntime {
    runtime: DefaultWorker,
    modules: HashMap<PathBuf, ModuleId>,
}

impl JsRuntime {
    pub fn try_new() -> Result<Self, LoadError> {
        let runtime = DefaultWorker::new(DefaultWorkerOptions {
            default_entrypoint: None,
            timeout: Duration::from_millis(100),
            ..Default::default()
        })?;
        let modules = HashMap::new();
        Ok(JsRuntime { runtime, modules })
    }

    #[cfg(test)]
    pub fn load_js(
        &mut self,
        path: impl AsRef<Path>,
        script: impl ToString,
    ) -> Result<JsFilter, LoadError> {
        let module = Module::new(path, script);
        self.load(module)
    }

    pub fn load_file(&mut self, path: impl AsRef<Path>) -> Result<JsFilter, LoadError> {
        let module = Module::load(path)?;
        self.load(module)
    }

    pub fn load(&mut self, module: Module) -> Result<JsFilter, LoadError> {
        let path = module.filename().to_owned();
        let module_id = self.runtime.load_module(module)?;

        self.modules.insert(path.clone(), module_id);
        Ok(JsFilter::new(path, module_id))
    }

    pub fn loaded_module(&self, path: PathBuf) -> Result<JsFilter, LoadError> {
        match self.modules.get(&path).cloned() {
            None => Err(LoadError::ScriptNotLoaded { path }),
            Some(module_id) => Ok(JsFilter::new(path, module_id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_filter() {
        let script = "export function process(t,msg) { return [msg]; };";
        let mut runtime = JsRuntime::try_new().unwrap();
        let filter = runtime.load_js("id.js", script).unwrap();

        let input = Message::new("te/main/device///m/", "hello world");
        let output = input.clone();
        assert_eq!(
            filter.process(&runtime, &DateTime::now(), &input).unwrap(),
            vec![output]
        );
    }

    #[test]
    fn error_filter() {
        let script = r#"export function process(t,msg) { throw new Error("Cannot process that message"); };"#;
        let mut runtime = JsRuntime::try_new().unwrap();
        let filter = runtime.load_js("err.js", script).unwrap();

        let input = Message::new("te/main/device///m/", "hello world");
        let error = filter
            .process(&runtime, &DateTime::now(), &input)
            .unwrap_err();
        assert!(error.to_string().contains("Cannot process that message"));
    }
}
