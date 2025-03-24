use crate::core::mapper::start_basic_actors;
use crate::TEdgeComponent;
use tedge_config::TEdgeConfig;
use tedge_wasm_mapper::WasmMapperBuilder;

pub struct WasmMapper;

#[async_trait::async_trait]
impl TEdgeComponent for WasmMapper {
    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        _config_dir: &tedge_config::Path,
    ) -> Result<(), anyhow::Error> {
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors("tedge-wasm-mapper", &tedge_config).await?;

        let mut wasm_mapper = WasmMapperBuilder::new("/etc/tedge/wasm-mapper");
        wasm_mapper.connect(&mut mqtt_actor);

        runtime.spawn(wasm_mapper).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}
