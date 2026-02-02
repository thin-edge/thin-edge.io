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
        flows.register_builtin(crate::mea::events::EventConverter::default());
        flows.register_builtin(crate::mea::alarms::AlarmConverter::default());
        flows.register_builtin(crate::mea::health::HealthStatusConverter::default());

        self.persist_builtin_flow(&mut flows).await?;
        Ok(flows)
    }

    async fn persist_builtin_flow(
        &self,
        flows: &mut ConnectedFlowRegistry,
    ) -> Result<(), UpdateFlowRegistryError> {
        let topic_prefix = &self.config.mqtt_schema.root;

        flows
            .persist_builtin_flow("units", self.units_flow().as_str())
            .await?;

        if self
            .config
            .topics
            .include_topic(&format!("{topic_prefix}/+/+/+/+/m/+"))
        {
            flows
                .persist_builtin_flow("measurements", self.measurements_flow().as_str())
                .await?;
        } else {
            flows.disable_builtin_flow("measurements").await?;
        }

        if self
            .config
            .topics
            .include_topic(&format!("{topic_prefix}/+/+/+/+/e/+"))
        {
            flows
                .persist_builtin_flow("events", self.events_flow().as_str())
                .await?;
        } else {
            flows.disable_builtin_flow("events").await?;
        }

        if self
            .config
            .topics
            .include_topic(&format!("{topic_prefix}/+/+/+/+/a/+"))
        {
            flows
                .persist_builtin_flow("alarms", self.alarms_flow().as_str())
                .await?
        } else {
            flows.disable_builtin_flow("alarms").await?;
        }

        if self
            .config
            .topics
            .include_topic(&format!("{topic_prefix}/+/+/+/+/status/health"))
        {
            flows
                .persist_builtin_flow("health", self.health_flow().as_str())
                .await?
        } else {
            flows.disable_builtin_flow("health").await?;
        }

        Ok(())
    }

    fn units_flow(&self) -> String {
        let mqtt_schema = &self.config.mqtt_schema;
        let topic_prefix = mqtt_schema.root.as_str();
        let errors_topic = mqtt_schema.error_topic();

        format!(
            r#"
input.mqtt.topics = ["{topic_prefix}/+/+/+/+/m/+/meta"]

steps = [
    {{ builtin = "update-context" }}
]

[output.mqtt]
topic = "{errors_topic}"

[errors.mqtt]
topic = "{errors_topic}"
"#
        )
    }

    fn measurements_flow(&self) -> String {
        let mqtt_schema = &self.config.mqtt_schema;
        let mapper_topic_id = &self.config.service_topic_id;
        let topic_prefix = mqtt_schema.root.as_str();
        let errors_topic = mqtt_schema.error_topic();
        let c8y_prefix = &self.config.bridge_config.c8y_prefix;
        let max_size = self.config.max_mqtt_payload_size;

        format!(
            r#"input.mqtt.topics = ["{topic_prefix}/+/+/+/+/m/+", "{topic_prefix}/{mapper_topic_id}/status/entities"]

steps = [
    {{ builtin = "add-timestamp", config = {{ property = "time", format = "unix", reformat = false }} }},
    {{ builtin = "into_c8y_measurements", config = {{ topic_root = "{topic_prefix}" }} }},
    {{ builtin = "limit-payload-size", config = {{ max_size = {max_size} }} }},
]

[output.mqtt]
topic = "{c8y_prefix}/measurement/measurements/create"

[errors.mqtt]
topic = "{errors_topic}"
"#
        )
    }

    fn events_flow(&self) -> String {
        let mqtt_schema = &self.config.mqtt_schema;
        let mapper_topic_id = &self.config.service_topic_id;
        let topic_prefix = mqtt_schema.root.as_str();
        let errors_topic = mqtt_schema.error_topic();
        let c8y_prefix = &self.config.bridge_config.c8y_prefix;
        let max_mqtt_payload_size = self.config.max_mqtt_payload_size;

        format!(
            r#"input.mqtt.topics = ["{topic_prefix}/+/+/+/+/e/+", "{topic_prefix}/{mapper_topic_id}/status/entities"]

steps = [
    {{ builtin = "add-timestamp", config = {{ property = "time", format = "rfc3339", reformat = false }} }},
    {{ builtin = "into_c8y_events", config = {{ topic_root = "{topic_prefix}", c8y_prefix = "{c8y_prefix}", max_mqtt_payload_size = {max_mqtt_payload_size} }} }},
]

[output.mqtt]

[errors.mqtt]
topic = "{errors_topic}"
"#
        )
    }

    fn alarms_flow(&self) -> String {
        let mqtt_schema = &self.config.mqtt_schema;
        let mapper_topic_id = &self.config.service_topic_id;
        let topic_prefix = mqtt_schema.root.as_str();
        let c8y_prefix = &self.config.bridge_config.c8y_prefix;
        let errors_topic = mqtt_schema.error_topic();
        let internal_alarms = crate::alarm_converter::INTERNAL_ALARMS_TOPIC;
        let max_size = self.config.max_mqtt_payload_size;

        format!(
            r#"input.mqtt.topics = ["{topic_prefix}/+/+/+/+/a/+", "{internal_alarms}#", "{topic_prefix}/{mapper_topic_id}/status/entities"]

steps = [
    {{ builtin = "add-timestamp", config = {{ property = "time", format = "rfc3339", reformat = false }} }},
    {{ builtin = "into_c8y_alarms", interval = "3s", config = {{ topic_root = "{topic_prefix}", c8y_prefix = "{c8y_prefix}" }} }},
    {{ builtin = "limit-payload-size", config = {{ max_size = {max_size} }} }},
]

[output.mqtt]

[errors.mqtt]
topic = "{errors_topic}"
"#
        )
    }

    fn health_flow(&self) -> String {
        let mqtt_schema = &self.config.mqtt_schema;
        let topic_prefix = mqtt_schema.root.as_str();
        let c8y_prefix = &self.config.bridge_config.c8y_prefix;
        let main_device = &self.config.device_topic_id;
        let mapper_topic_id = &self.config.service_topic_id;
        let errors_topic = mqtt_schema.error_topic();

        format!(
            r#"
input.mqtt.topics = ["{topic_prefix}/+/+/+/+/status/health", "{topic_prefix}/{mapper_topic_id}/status/entities"]

steps = [
    {{ builtin = "into_c8y_health_status", config = {{ topic_root = "{topic_prefix}", main_device = "{main_device}", c8y_prefix = "{c8y_prefix}" }} }},
]

[output.mqtt]

[errors.mqtt]
topic = "{errors_topic}"
"#
        )
    }
}
