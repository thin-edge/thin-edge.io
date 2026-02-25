use crate::js_lib;
use crate::js_lib::kv_store::FlowContextHandle;
use crate::js_script::JsScript;
use crate::js_value::JsonValue;
use crate::LoadError;
use anyhow::anyhow;
use camino::Utf8Path;
use rquickjs::module::Evaluated;
use rquickjs::CaughtError;
use rquickjs::Ctx;
use rquickjs::Module;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::debug;

pub struct JsRuntime {
    runtime: rquickjs::AsyncRuntime,
    store: FlowContextHandle,
    worker: mpsc::Sender<JsRequest>,
    execution_timeout: Duration,
}

static TIME_CREDITS: AtomicUsize = AtomicUsize::new(1000);

impl JsRuntime {
    pub async fn try_new(store: FlowContextHandle) -> Result<Self, LoadError> {
        let runtime = rquickjs::AsyncRuntime::new()?;
        runtime.set_memory_limit(16 * 1024 * 1024).await;
        runtime.set_max_stack_size(256 * 1024).await;
        runtime
            .set_interrupt_handler(Some(Box::new(|| {
                let credits = TIME_CREDITS.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                credits == 0
            })))
            .await;
        let context = rquickjs::AsyncContext::full(&runtime).await?;
        let worker = JsWorker::spawn(context, store.clone()).await;
        let execution_timeout = Duration::from_secs(5);
        Ok(JsRuntime {
            runtime,
            store,
            worker,
            execution_timeout,
        })
    }

    pub fn context_handle(&self) -> FlowContextHandle {
        self.store.clone()
    }

    pub async fn load_script(&mut self, script: &mut JsScript) -> Result<(), LoadError> {
        let exports = self
            .load_file(script.module_name.to_owned(), script.path())
            .await?;
        Self::set_exports(script, &exports);
        Ok(())
    }

    pub async fn load_script_literal(
        &mut self,
        script: &mut JsScript,
        source: impl Into<Vec<u8>>,
    ) -> Result<(), LoadError> {
        let exports = self.load_js(script.module_name.to_owned(), source).await?;
        Self::set_exports(script, &exports);
        Ok(())
    }

    fn set_exports(script: &mut JsScript, exports: &[&str]) {
        for export in exports {
            match *export {
                "onMessage" => script.is_defined = true,
                "onInterval" => script.is_periodic = true,
                _ => (),
            }
        }
        if !script.is_defined {
            tracing::warn!(target: "flows", "Flow script with no 'onMessage' function: {}", script.path);
        }
    }

    async fn load_file(
        &mut self,
        module_name: String,
        path: impl AsRef<Utf8Path>,
    ) -> Result<Vec<&'static str>, LoadError> {
        let path = path.as_ref();
        let source = tokio::fs::read_to_string(path)
            .await
            .map_err(|err| LoadError::from_io(err, path))?;
        self.load_js(module_name, source).await
    }

    async fn load_js(
        &mut self,
        name: String,
        source: impl Into<Vec<u8>>,
    ) -> Result<Vec<&'static str>, LoadError> {
        let (sender, receiver) = oneshot::channel();
        let source = source.into();
        let imports = vec!["onMessage", "onInterval"];
        TIME_CREDITS.store(100000, std::sync::atomic::Ordering::Relaxed);
        self.send(
            receiver,
            JsRequest::LoadModule {
                name,
                source,
                imports,
                sender,
            },
        )
        .await?
    }

    pub async fn call_function(
        &self,
        module: &str,
        function: &str,
        args: Vec<JsonValue>,
    ) -> Result<JsonValue, LoadError> {
        let (sender, receiver) = oneshot::channel();
        TIME_CREDITS.store(1000, std::sync::atomic::Ordering::Relaxed);
        self.send(
            receiver,
            JsRequest::CallFunction {
                module: module.to_string(),
                function: function.to_string(),
                args,
                sender,
            },
        )
        .await?
    }

    pub async fn dump_memory_stats(&self) -> serde_json::Value {
        let usage = self.runtime.memory_usage().await;
        serde_json::json!({
            "malloc_bytes": usage.malloc_size,
            "memory_used_bytes": usage.memory_used_size,
            "function_count": usage.js_func_count,
            "object_count": usage.obj_count,
        })
    }

    async fn send<Response>(
        &self,
        mut receiver: oneshot::Receiver<Response>,
        request: JsRequest,
    ) -> Result<Response, anyhow::Error> {
        self.worker
            .send(request)
            .await
            .map_err(|err| anyhow!(err))?;

        // FIXME: The following timeout is not working
        //  - see unit test: js_script::while_loop
        //  - the issue is that the quickjs runtime fails to yield when executing `while(true)`
        //  - Using task::spawn_blocking to launch the quickjs runtime doesn't help
        //    - A timeout is the properly raised
        //    - but the JS runtime keeps executing `while(true)` and is no more responsive.
        match tokio::time::timeout(self.execution_timeout, &mut receiver).await {
            Ok(response) => response.map_err(|err| anyhow!(err)),
            Err(_) => Err(anyhow!("Maximum processing time exceeded")),
        }
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
    pub async fn spawn(
        context: rquickjs::AsyncContext,
        store: FlowContextHandle,
    ) -> mpsc::Sender<JsRequest> {
        let (sender, requests) = mpsc::channel(100);
        tokio::spawn(async move {
            let worker = JsWorker { context, requests };
            worker.run(store).await
        });
        sender
    }

    async fn run(mut self, store: FlowContextHandle) {
        rquickjs::async_with!(self.context => |ctx| {
            js_lib::console::init(&ctx);
            js_lib::text_decoder::init(&ctx);
            js_lib::text_encoder::init(&ctx);
            store.init(&ctx);
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
        debug!(target: "flows", "compile({name})");
        let module = Module::declare(ctx.clone(), name.clone(), source)
            .map_err(|err| LoadError::from_js(&ctx, err))?;
        let (module, p) = module.eval().map_err(|err| LoadError::from_js(&ctx, err))?;
        let () = p.finish().map_err(|err| LoadError::from_js(&ctx, err))?;

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
        debug!(target: "flows", "link({module_name}.{function})");
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
            _ => return Err(anyhow::anyhow!("Too many args").into()),
        };

        debug!(target: "flows", "execute({module_name}.{function}) => {r:?}");
        r.map_err(|err| LoadError::from_js(&ctx, err))
    }
}

impl LoadError {
    fn from_js(ctx: &Ctx<'_>, err: rquickjs::Error) -> Self {
        if let Some(ex) = ctx.catch().as_exception() {
            let err = anyhow::anyhow!("{ex}");
            err.context("JS raised exception").into()
        } else {
            let err = CaughtError::from_error(ctx, err);
            let err = anyhow::anyhow!("{err}");
            err.context("JS runtime error").into()
        }
    }
}
