use crate::wasm::Message;
use crate::wasm::Timestamp;
use crate::wasm::WasmFilter;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use wasmtime::component::Component;
use wasmtime::component::Linker;
use wasmtime::component::ResourceTable;
use wasmtime::Engine;
use wasmtime::Store;
use wasmtime_wasi::IoView;
use wasmtime_wasi::WasiCtx;
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::WasiView;

pub struct HostEngine {
    engine: Engine,
    linker: Linker<HostState>,
    components: HashMap<Utf8PathBuf, Component>,
}

impl HostEngine {
    pub fn try_new() -> Result<Self, LoadError> {
        let engine = Engine::default();
        let mut linker = <Linker<HostState>>::new(&engine);
        wasmtime_wasi::add_to_linker_sync(&mut linker)?;

        Ok(HostEngine {
            engine,
            linker,
            components: HashMap::default(),
        })
    }

    pub async fn load_component(&mut self, file: impl AsRef<Utf8Path>) -> Result<(), LoadError> {
        let path = file.as_ref();
        let bytes = tokio::fs::read(path).await?;
        let component = Component::new(&self.engine, bytes)?;
        self.components.insert(path.to_path_buf(), component);
        Ok(())
    }

    pub fn instantiate(&self, component: &Utf8Path) -> Result<WasmFilter, LoadError> {
        let Some(component) = self.components.get(component) else {
            return Err(LoadError::FileNotFound {
                path: component.into(),
            });
        };

        let state = HostState::default();
        let mut store = Store::new(&self.engine, state);
        let instance = self.linker.instantiate(&mut store, component)?;
        let process_func = instance
            .get_typed_func::<(Timestamp, Message), (crate::wasm::TransformedMessages,)>(
                &mut store, "process",
            )?;

        Ok(WasmFilter::new(store, process_func))
    }
}

pub struct HostState {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl IoView for HostState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}
impl WasiView for HostState {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

impl Default for HostState {
    fn default() -> HostState {
        let mut wasi = WasiCtxBuilder::new();

        HostState {
            ctx: wasi.build(),
            table: ResourceTable::new(),
        }
    }
}
