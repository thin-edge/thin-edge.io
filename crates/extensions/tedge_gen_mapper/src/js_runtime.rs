use crate::js_filter::JsFilter;
use crate::js_filter::JsonValue;
use crate::LoadError;
use anyhow::anyhow;
use rquickjs::module::Evaluated;
use rquickjs::Ctx;
use rquickjs::Module;
use std::collections::HashMap;
use std::path::Path;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::debug;

pub struct JsRuntime {
    runtime: rquickjs::AsyncRuntime,
    worker: mpsc::Sender<JsRequest>,
}

impl JsRuntime {
    pub async fn try_new() -> Result<Self, LoadError> {
        let runtime = rquickjs::AsyncRuntime::new()?;
        let context = rquickjs::AsyncContext::full(&runtime).await?;
        let worker = JsWorker::spawn(context).await;
        Ok(JsRuntime { runtime, worker })
    }

    pub async fn load_filter(&mut self, filter: &mut JsFilter) -> Result<(), LoadError> {
        let exports = self.load_file(filter.module_name(), filter.path()).await?;
        for export in exports {
            match export {
                "process" => filter.no_js_process = false,
                "update_config" => filter.no_js_update_config = false,
                "tick" => filter.no_js_tick = false,
                _ => (),
            }
        }
        Ok(())
    }

    pub async fn load_file(
        &mut self,
        module_name: String,
        path: impl AsRef<Path>,
    ) -> Result<Vec<&'static str>, LoadError> {
        let path = path.as_ref();
        let source = tokio::fs::read_to_string(path).await?;
        self.load_js(module_name, source).await
    }

    pub async fn load_js(
        &mut self,
        name: String,
        source: impl Into<Vec<u8>>,
    ) -> Result<Vec<&'static str>, LoadError> {
        let (sender, receiver) = oneshot::channel();
        let source = source.into();
        let imports = vec!["process", "update_config", "tick"];
        self.worker
            .send(JsRequest::LoadModule {
                name,
                source,
                imports,
                sender,
            })
            .await
            .map_err(|err| anyhow!(err))?;
        receiver.await.map_err(|err| anyhow!(err))?
    }

    pub async fn call_function(
        &self,
        module: &str,
        function: &str,
        args: Vec<JsonValue>,
    ) -> Result<JsonValue, LoadError> {
        let (sender, receiver) = oneshot::channel();
        self.worker
            .send(JsRequest::CallFunction {
                module: module.to_string(),
                function: function.to_string(),
                args,
                sender,
            })
            .await
            .map_err(|err| anyhow!(err))?;
        receiver.await.map_err(|err| anyhow!(err))?
    }

    pub async fn dump_memory_stats(&self) {
        let usage = self.runtime.memory_usage().await;
        tracing::info!(target: "gen-mapper", "Memory usage:");
        tracing::info!(target: "gen-mapper", "  - malloc size: {}", usage.malloc_size);
        tracing::info!(target: "gen-mapper", "  - used memory size: {}", usage.memory_used_size);
        tracing::info!(target: "gen-mapper", "  - function count: {}", usage.js_func_count);
        tracing::info!(target: "gen-mapper", "  - object count: {}", usage.obj_count);
        tracing::info!(target: "gen-mapper", "  - array count: {}", usage.array_count);
        tracing::info!(target: "gen-mapper", "  - string count: {}", usage.str_count);
        tracing::info!(target: "gen-mapper", "  - atom count: {}", usage.atom_count);
    }
}

enum JsRequest {
    LoadModule {
        name: String,
        source: Vec<u8>,
        imports: Vec<&'static str>,
        sender: oneshot::Sender<Result<Vec<&'static str>, LoadError>>,
    },
    CallFunction {
        module: String,
        function: String,
        args: Vec<JsonValue>,
        sender: oneshot::Sender<Result<JsonValue, LoadError>>,
    },
}

struct JsWorker {
    context: rquickjs::AsyncContext,
    requests: mpsc::Receiver<JsRequest>,
}

impl JsWorker {
    pub async fn spawn(context: rquickjs::AsyncContext) -> mpsc::Sender<JsRequest> {
        let (sender, requests) = mpsc::channel(100);
        tokio::spawn(async move {
            let worker = JsWorker { context, requests };
            worker.run().await
        });
        sender
    }

    async fn run(mut self) {
        rquickjs::async_with!(self.context => |ctx| {
            console::init(&ctx);
            let mut modules = JsModules::new();
            while let Some(request) = self.requests.recv().await {
                match request {
                    JsRequest::LoadModule{name, source, sender, imports} => {
                        let result = modules.load_module(ctx.clone(), name, source, imports).await;
                        let _ = sender.send(result);
                    }
                    JsRequest::CallFunction{module, function, args, sender} => {
                        let result = modules.call_function(ctx.clone(), module, function, args).await;
                        let _ = sender.send(result);
                    }
                }
            }
        })
        .await
    }
}

struct JsModules<'js> {
    modules: HashMap<String, Module<'js, Evaluated>>,
}

impl<'js> JsModules<'js> {
    fn new() -> Self {
        JsModules {
            modules: HashMap::new(),
        }
    }

    async fn load_module(
        &mut self,
        ctx: Ctx<'js>,
        name: String,
        source: Vec<u8>,
        imports: Vec<&'static str>,
    ) -> Result<Vec<&'static str>, LoadError> {
        debug!(target: "MAPPING", "compile({name})");
        let module = Module::declare(ctx, name.clone(), source)?;
        let (module, p) = module.eval()?;
        let () = p.finish()?;

        let mut exports = vec![];
        for import in imports {
            if let Ok(Some(v)) = module.get(import) {
                if rquickjs::Function::from_value(v).is_ok() {
                    exports.push(import);
                }
            }
        }

        self.modules.insert(name, module);
        Ok(exports)
    }

    async fn call_function(
        &mut self,
        ctx: Ctx<'js>,
        module_name: String,
        function: String,
        args: Vec<JsonValue>,
    ) -> Result<JsonValue, LoadError> {
        debug!(target: "MAPPING", "link({module_name}.{function})");
        let module = self
            .modules
            .get(&module_name)
            .ok_or_else(|| LoadError::UnknownModule {
                module_name: module_name.clone(),
            })?;
        let f: rquickjs::Value = module
            .get(&function)
            .map_err(|_| LoadError::UnknownFunction {
                module_name: module_name.clone(),
                function: function.clone(),
            })?;
        let f = rquickjs::Function::from_value(f).map_err(|_| LoadError::UnknownFunction {
            module_name: module_name.clone(),
            function: function.clone(),
        })?;

        let r = match &args[..] {
            [] => f.call(()),
            [v0] => f.call((v0,)),
            [v0, v1] => f.call((v0, v1)),
            [v0, v1, v2] => f.call((v0, v1, v2)),
            [v0, v1, v2, v3] => f.call((v0, v1, v2, v3)),
            [v0, v1, v2, v3, v4] => f.call((v0, v1, v2, v3, v4)),
            [v0, v1, v2, v3, v4, v5] => f.call((v0, v1, v2, v3, v4, v5)),
            [v0, v1, v2, v3, v4, v5, v6] => f.call((v0, v1, v2, v3, v4, v5, v6)),
            _ => return Err(anyhow::anyhow!("Too many args").into()),
        };

        debug!(target: "MAPPING", "execute({module_name}.{function}) => {r:?}");
        r.map_err(|err| {
            if let Some(ex) = ctx.catch().as_exception() {
                let err = anyhow::anyhow!("{ex}");
                err.context("JS raised exception").into()
            } else {
                debug!(target: "MAPPING", "execute({module_name}.{function}) => {err:?}");
                err.into()
            }
        })
    }
}

mod console {
    use crate::js_filter::JsonValue;
    use rquickjs::class::Trace;
    use rquickjs::function::Rest;
    use rquickjs::Ctx;
    use rquickjs::JsLifetime;
    use rquickjs::Result;
    use rquickjs::Value;
    use std::fmt::Write;

    #[derive(Clone, Trace, JsLifetime)]
    #[rquickjs::class(frozen)]
    struct Console {}

    pub fn init(ctx: &Ctx<'_>) {
        let console = Console {};
        let _ = ctx.globals().set("console", console);
    }

    impl Console {
        fn print(&self, _level: tracing::Level, values: Rest<Value<'_>>) -> Result<()> {
            let mut message = String::new();
            for (i, value) in values.0.into_iter().enumerate() {
                if i > 0 {
                    let _ = write!(&mut message, ", ");
                }
                let _ = write!(&mut message, "{}", JsonValue::display(value));
            }
            eprintln!("JavaScript.Console: {message}");
            Ok(())
        }
    }

    #[rquickjs::methods]
    impl Console {
        fn debug(&self, values: Rest<Value<'_>>) -> Result<()> {
            self.print(tracing::Level::DEBUG, values)
        }

        fn log(&self, values: Rest<Value<'_>>) -> Result<()> {
            self.print(tracing::Level::INFO, values)
        }

        fn warn(&self, values: Rest<Value<'_>>) -> Result<()> {
            self.print(tracing::Level::WARN, values)
        }

        fn error(&self, values: Rest<Value<'_>>) -> Result<()> {
            self.print(tracing::Level::ERROR, values)
        }
    }
}
