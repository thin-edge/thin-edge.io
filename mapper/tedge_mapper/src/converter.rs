use mqtt_client::{Message, Topic, TopicFilter};
use std::fmt::Display;
use tracing::error;

#[derive(Debug)]
pub struct MapperConfig {
    pub in_topic_filter: TopicFilter,
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

pub trait Converter: Send + Sync {
    type Error: Display;

    fn get_mapper_config(&self) -> &MapperConfig;

    fn get_in_topic_filter(&self) -> &TopicFilter {
        &self.get_mapper_config().in_topic_filter
    }

    fn convert_messages(&mut self, input: &Message) -> Result<Vec<Message>, Self::Error>;

    fn convert(&mut self, input: &Message) -> Vec<Message> {
        match self.convert_messages(input) {
            Ok(messages) => messages,
            Err(error) => {
                error!("Mapping error: {}", error);
                vec![Message::new(
                    &self.get_mapper_config().errors_topic,
                    error.to_string(),
                )]
            }
        }
    }
}

pub fn make_valid_topic_or_panic(topic_name: &str) -> Topic {
    Topic::new(topic_name).expect("Invalid topic name")
}

pub fn make_valid_topic_filter_or_panic(filter_name: &str) -> TopicFilter {
    TopicFilter::new(filter_name).expect("Invalid topic filter name")
}
