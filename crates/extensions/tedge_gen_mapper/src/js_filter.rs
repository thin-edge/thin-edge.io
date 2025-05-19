use crate::pipeline;
use crate::pipeline::DateTime;
use crate::pipeline::FilterError;
use crate::pipeline::Message;
use crate::LoadError;
use rustyscript::deno_core::ModuleId;
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
}

impl JsFilter {
    pub fn process(
        &self,
        js: &JsRuntime,
        timestamp: &DateTime,
        message: &Message,
    ) -> Result<Vec<Message>, FilterError> {
        debug!(target: "MAPPING", "{}: process({timestamp:?}, {message:?})", self.path.display());
        let input = vec![timestamp.json(), message.json()];
        js.runtime
            .call_function(Some(self.module_id), "process".to_string(), input)
            .map_err(pipeline::error_from_js)
    }

    pub fn update_config(&self, _js: &JsRuntime, config: &Message) -> Result<(), FilterError> {
        debug!(target: "MAPPING", "{}: update_config({config:?})", self.path.display());
        Ok(())
    }

    pub fn tick(&self, _js: &JsRuntime, timestamp: &DateTime) -> Result<Vec<Message>, FilterError> {
        debug!(target: "MAPPING", "{}: tick({timestamp:?})", self.path.display());
        Ok(vec![])
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
        Ok(JsFilter { path, module_id })
    }

    pub fn loaded_module(&self, path: PathBuf) -> Result<JsFilter, LoadError> {
        match self.modules.get(&path).cloned() {
            None => Err(LoadError::ScriptNotLoaded { path }),
            Some(module_id) => Ok(JsFilter { path, module_id }),
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
