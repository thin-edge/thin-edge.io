use crate::actor::C8yMapperBuilder;
use camino::Utf8Path;
use tedge_flows::ConnectedFlowRegistry;
use tedge_flows::FlowRegistryExt;
use tedge_flows::UpdateFlowRegistryError;
use tedge_mqtt_ext::TopicFilter;
use tedge_utils::file::create_directory_with_defaults;
use tracing::error;

impl C8yMapperBuilder {
    pub async fn flow_registry(
        &self,
        flows_dir: impl AsRef<Utf8Path>,
    ) -> Result<ConnectedFlowRegistry, UpdateFlowRegistryError> {
        if let Err(err) = create_directory_with_defaults(flows_dir.as_ref()).await {
            error!(
                "failed to create flow directory '{}': {err}",
                flows_dir.as_ref()
            );
            return Err(err)?;
        };
        let mut flows = ConnectedFlowRegistry::new(flows_dir);

        let mapper_topic_id = self.config.service_topic_id.clone();
        flows.register_builtin(crate::mea::message_cache::MessageCache::new(
            mapper_topic_id,
        ));
        flows.register_builtin(crate::mea::measurements::MeasurementConverter::default());
        flows.register_builtin(crate::mea::events::EventConverter::default());
        flows.register_builtin(crate::mea::alarms::AlarmConverter::default());
        flows.register_builtin(crate::mea::health::HealthStatusConverter::default());

        self.persist_builtin_flows(&mut flows).await?;
        Ok(flows)
    }

    async fn persist_builtin_flows(
        &self,
        flows: &mut ConnectedFlowRegistry,
    ) -> Result<(), UpdateFlowRegistryError> {
        flows
            .persist_builtin_flow("units", &self.units_flow())
            .await?;

        flows
            .persist_builtin_flow("measurements", &self.measurements_flow())
            .await?;

        flows
            .persist_builtin_flow("events", &self.events_flow())
            .await?;

        flows
            .persist_builtin_flow("alarms", &self.alarms_flow())
            .await?;

        flows
            .persist_builtin_flow("health", &self.health_flow())
            .await?;

        Ok(())
    }

    fn configured_topics(&self, filter: &str) -> Vec<String> {
        let filter = TopicFilter::new_unchecked(filter);
        self.config
            .topics
            .patterns()
            .iter()
            .filter(|topic| filter.accept_topic_name(topic))
            .cloned()
            .collect()
    }

    fn units_flow(&self) -> String {
        let mqtt_schema = &self.config.mqtt_schema;
        let topic_prefix = mqtt_schema.root.as_str();
        let errors_topic = mqtt_schema.error_topic();
        let input_topics = self.configured_topics(&format!("{topic_prefix}/+/+/+/+/m/+/meta"));

        format!(
            r#"
input.mqtt.topics = {input_topics:?}

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
        let mut input_topics = self.configured_topics(&format!("{topic_prefix}/+/+/+/+/m/+"));
        if !input_topics.is_empty() {
            input_topics.push(format!("{topic_prefix}/{mapper_topic_id}/status/entities"));
        }

        format!(
            r#"input.mqtt.topics = {input_topics:?}

steps = [
    {{ builtin = "add-timestamp", config = {{ property = "time", format = "unix", reformat = false }} }},
    {{ builtin = "cache-early-messages", config = {{ topic_root = "{topic_prefix}" }} }},
    {{ builtin = "into-c8y-measurements", config = {{ topic_root = "{topic_prefix}" }} }},
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
        let mut input_topics = self.configured_topics(&format!("{topic_prefix}/+/+/+/+/e/+"));
        if !input_topics.is_empty() {
            input_topics.push(format!("{topic_prefix}/{mapper_topic_id}/status/entities"));
        }

        format!(
            r#"input.mqtt.topics = {input_topics:?}

steps = [
    {{ builtin = "add-timestamp", config = {{ property = "time", format = "rfc3339", reformat = false }} }},
    {{ builtin = "cache-early-messages", config = {{ topic_root = "{topic_prefix}" }} }},
    {{ builtin = "into-c8y-events", config = {{ topic_root = "{topic_prefix}", c8y_prefix = "{c8y_prefix}", max_mqtt_payload_size = {max_mqtt_payload_size} }} }},
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
        let mut input_topics = self.configured_topics(&format!("{topic_prefix}/+/+/+/+/a/+"));
        if !input_topics.is_empty() {
            input_topics.push(format!("{internal_alarms}#"));
            input_topics.push(format!("{topic_prefix}/{mapper_topic_id}/status/entities"));
        }

        format!(
            r#"input.mqtt.topics = {input_topics:?}

steps = [
    {{ builtin = "add-timestamp", config = {{ property = "time", format = "rfc3339", reformat = false }} }},
    {{ builtin = "cache-early-messages", config = {{ topic_root = "{topic_prefix}" }} }},
    {{ builtin = "into-c8y-alarms", interval = "3s", config = {{ topic_root = "{topic_prefix}", c8y_prefix = "{c8y_prefix}" }} }},
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
        let mut input_topics =
            self.configured_topics(&format!("{topic_prefix}/+/+/+/+/status/health"));
        if !input_topics.is_empty() {
            input_topics.push(format!("{topic_prefix}/{mapper_topic_id}/status/entities"));
        }

        format!(
            r#"input.mqtt.topics = {input_topics:?}

steps = [
    {{ builtin = "cache-early-messages", config = {{ topic_root = "{topic_prefix}" }} }},
    {{ builtin = "into-c8y-health-status", config = {{ topic_root = "{topic_prefix}", main_device = "{main_device}", c8y_prefix = "{c8y_prefix}" }} }},
]

[output.mqtt]

[errors.mqtt]
topic = "{errors_topic}"
"#
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::test_mapper_config;
    use crate::tests::TestHandleBuilder;
    use tedge_flows::FlowConfig;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn check_measurements_flow() {
        let tmp_dir = TempTedgeDir::new();
        let TestHandleBuilder { c8y, .. } = c8y_mapper_builder(&tmp_dir).await;
        let flow_specs = c8y.measurements_flow();

        let _flow: FlowConfig = toml::from_str(&flow_specs).unwrap();
    }

    #[tokio::test]
    async fn check_units_flow() {
        let tmp_dir = TempTedgeDir::new();
        let TestHandleBuilder { c8y, .. } = c8y_mapper_builder(&tmp_dir).await;
        let flow_specs = c8y.units_flow();

        let _flow: FlowConfig = toml::from_str(&flow_specs).unwrap();
    }

    #[tokio::test]
    async fn check_events_flow() {
        let tmp_dir = TempTedgeDir::new();
        let TestHandleBuilder { c8y, .. } = c8y_mapper_builder(&tmp_dir).await;
        let flow_specs = c8y.events_flow();

        let _flow: FlowConfig = toml::from_str(&flow_specs).unwrap();
    }

    #[tokio::test]
    async fn check_alarms_flow() {
        let tmp_dir = TempTedgeDir::new();
        let TestHandleBuilder { c8y, .. } = c8y_mapper_builder(&tmp_dir).await;
        let flow_specs = c8y.alarms_flow();

        let _flow: FlowConfig = toml::from_str(&flow_specs).unwrap();
    }

    #[tokio::test]
    async fn check_health_flow() {
        let tmp_dir = TempTedgeDir::new();
        let TestHandleBuilder { c8y, .. } = c8y_mapper_builder(&tmp_dir).await;
        let flow_specs = c8y.health_flow();

        let _flow: FlowConfig = toml::from_str(&flow_specs).unwrap();
    }

    async fn c8y_mapper_builder(tmp_dir: &TempTedgeDir) -> TestHandleBuilder {
        let config = test_mapper_config(tmp_dir);
        crate::tests::c8y_mapper_builder(tmp_dir, config, true).await
    }
}
