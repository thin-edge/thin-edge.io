use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::models::timestamp::TimeFormat;
use tedge_config::models::TopicPrefix;
use tedge_mqtt_ext::Topic;

pub struct AzureConverter {
    input_topics: String,
    output_topic: Topic,
    errors_topic: Topic,
    add_timestamp: bool,
    time_format: TimeFormat,
    size_threshold: usize,
}

impl AzureConverter {
    pub fn new(
        add_timestamp: bool,
        mqtt_schema: MqttSchema,
        time_format: TimeFormat,
        topic_prefix: &TopicPrefix,
        max_payload_size: u32,
        input_topics: String,
    ) -> Self {
        let output_topic = Topic::new_unchecked(&format!("{topic_prefix}/messages/events/"));
        let errors_topic = mqtt_schema.error_topic();
        let size_threshold = max_payload_size as usize;
        AzureConverter {
            input_topics,
            output_topic,
            errors_topic,
            add_timestamp,
            time_format,
            size_threshold,
        }
    }

    pub fn builtin_flow(&self) -> String {
        let timestamp_step = if self.add_timestamp {
            format!(
                r#"{{ builtin = "add-timestamp", config = {{ property = "time", format = "{time_format}", reformat = true }} }},"#,
                time_format = self.time_format,
            )
        } else {
            "".to_string()
        };

        format!(
            r#"
input.mqtt.topics = {input_topics}

steps = [
    {{ builtin = "skip-mosquitto-health-status" }},
    {timestamp_step}
    {{ builtin = "cap-payload-size", config = {{ max_size = {max_size} }} }},
]

output.mqtt.topic = "{output_topic}"
errors.mqtt.topic = "{errors_topic}"
"#,
            input_topics = self.input_topics,
            max_size = self.size_threshold,
            output_topic = self.output_topic,
            errors_topic = self.errors_topic,
        )
    }
}
