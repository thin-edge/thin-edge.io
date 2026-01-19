use crate::actor::C8yMapperBuilder;
use camino::Utf8Path;
use tedge_flows::ConnectedFlowRegistry;
use tedge_flows::UpdateFlowRegistryError;
use tedge_utils::file::create_directory_with_defaults;

impl C8yMapperBuilder {
    pub async fn flow_registry(
        &self,
        flows_dir: impl AsRef<Utf8Path>,
    ) -> Result<ConnectedFlowRegistry, UpdateFlowRegistryError> {
        create_directory_with_defaults(flows_dir.as_ref()).await?;
        Ok(ConnectedFlowRegistry::new(flows_dir))
    }
}
