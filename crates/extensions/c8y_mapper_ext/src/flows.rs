use crate::actor::C8yMapperBuilder;
use camino::Utf8Path;
use tedge_flows::ConnectedFlowRegistry;
use tedge_flows::FlowRegistryExt;
use tedge_flows::UpdateFlowRegistryError;
use tedge_utils::file::create_directory_with_defaults;

impl C8yMapperBuilder {
    pub async fn flow_registry(
        &self,
        flows_dir: impl AsRef<Utf8Path>,
    ) -> Result<ConnectedFlowRegistry, UpdateFlowRegistryError> {
        create_directory_with_defaults(flows_dir.as_ref()).await?;
        let mut flows = ConnectedFlowRegistry::new(flows_dir);
        flows.register_builtin(crate::mea::measurements::MeasurementConverter::default());
        self.persist_builtin_flow(&mut flows).await?;
        Ok(flows)
    }

    async fn persist_builtin_flow(
        &self,
        flows: &mut ConnectedFlowRegistry,
    ) -> Result<(), UpdateFlowRegistryError> {
        flows
            .persist_builtin_flow("units", self.units_flow().as_str())
            .await?;
        flows
            .persist_builtin_flow("measurements", self.measurements_flow().as_str())
            .await
    }

    fn units_flow(&self) -> String {
        let mqtt_schema = &self.config.mqtt_schema;
        let topic_prefix = mqtt_schema.root.as_str();
        let errors_topic = mqtt_schema.error_topic();

        format!(
            r#"
[input.mqtt]
topics = ["{topic_prefix}/+/+/+/+/m/+/meta"]

[output.context]

[errors.mqtt]
topic = "{errors_topic}"
"#
        )
    }

    fn measurements_flow(&self) -> String {
        let mqtt_schema = &self.config.mqtt_schema;
        let topic_prefix = mqtt_schema.root.as_str();
        let errors_topic = mqtt_schema.error_topic();
        let c8y_prefix = &self.config.bridge_config.c8y_prefix;
        let max_size = self.config.max_mqtt_payload_size;

        format!(
            r#"input.mqtt.topics = ["{topic_prefix}/+/+/+/+/m/+"]

steps = [
    {{ builtin = "add-timestamp", config = {{ property = "time", format = "unix", reformat = false }} }},
    {{ builtin = "into_c8y_measurements" }},
    {{ builtin = "limit-payload-size", config = {{ max_size = {max_size} }} }},
]

[output.mqtt]
topic = "{c8y_prefix}/measurement/measurements/create"

[errors.mqtt]
topic = "{errors_topic}"
"#
        )
    }
}
