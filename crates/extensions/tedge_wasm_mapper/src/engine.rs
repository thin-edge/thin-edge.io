use crate::pipeline::Filter;
use crate::wasm::Datetime;
use crate::wasm::Message;
use crate::wasm::TransformedMessages;
use crate::wasm::WasmFilter;
use crate::wasm_filter::WasmFilterResource;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
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

    pub fn instantiate(&self, component: &Utf8Path) -> Result<Box<dyn Filter>, LoadError> {
        self.instantiate_filter(component)
            .map(WasmFilter::into_dyn)
            .or_else(|_| {
                // FIXME: should read the config file
                let config_topic = Topic::new_unchecked("config");
                let no_config = MqttMessage::new(&config_topic, "");
                self.instantiate_resource(component, &no_config)
                    .map(WasmFilterResource::into_dyn)
            })
    }

    pub fn instantiate_filter(&self, component: &Utf8Path) -> Result<WasmFilter, LoadError> {
        let Some(component) = self.components.get(component) else {
            return Err(LoadError::FileNotFound {
                path: component.into(),
            });
        };

        let state = HostState::default();
        let mut store = Store::new(&self.engine, state);
        let instance = self.linker.instantiate(&mut store, component)?;
        let process_func = instance
            .get_typed_func::<(Datetime, Message), (TransformedMessages,)>(&mut store, "process")
            .map_err(|error| LoadError::WasmFailedImport {
                import: "'process' function".to_owned(),
                error,
            })?;

        Ok(WasmFilter::new(store, process_func))
    }

    pub fn instantiate_resource(
        &self,
        component: &Utf8Path,
        config: &MqttMessage,
    ) -> Result<WasmFilterResource, LoadError> {
        let Some(component) = self.components.get(component) else {
            return Err(LoadError::FileNotFound {
                path: component.into(),
            });
        };

        let state = HostState::default();
        let store = Store::new(&self.engine, state);
        WasmFilterResource::try_new(store, component, &self.linker, config)
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
