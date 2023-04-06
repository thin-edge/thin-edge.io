use crate::batcher::MessageBatch;
use crate::collectd::CollectdMessage;
use batcher::BatchDriverOutput;
use log::error;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;

pub fn batch_into_mqtt_messages(
    output_topic: &Topic,
    in_message: BatchDriverOutput<CollectdMessage>,
) -> Vec<MqttMessage> {
    match in_message {
        BatchDriverOutput::Batch(measurements) => {
            match MessageBatch::thin_edge_json(output_topic, measurements) {
                Ok(message) => {
                    vec![message]
                }
                Err(err) => {
                    error!("Error while encoding a thin-edge json message: {}", err);
                    vec![]
                }
            }
        }
        BatchDriverOutput::Flush => vec![],
    }
}
