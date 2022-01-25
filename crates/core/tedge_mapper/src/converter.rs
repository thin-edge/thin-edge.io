use mqtt_channel::{Message, Topic, TopicFilter};
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

    fn try_convert(&mut self, input: &Message) -> Result<Vec<Message>, Self::Error>;

    fn convert(&mut self, input: &Message) -> Vec<Message> {
        let messages_or_err = self.try_convert(input);
        self.wrap_error(messages_or_err)
    }

    fn wrap_error(&self, messages_or_err: Result<Vec<Message>, Self::Error>) -> Vec<Message> {
        match messages_or_err {
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

    fn try_init_messages(&self) -> Result<Vec<Message>, Self::Error> {
        Ok(vec![])
    }

    fn init_messages(&self) -> Vec<Message> {
        match self.try_init_messages() {
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

    fn sync_messages(&mut self) -> Vec<Message> {
        vec![]
    }
}

pub fn make_valid_topic_or_panic(topic_name: &str) -> Topic {
    Topic::new(topic_name).expect("Invalid topic name")
}

pub fn make_valid_topic_filter_or_panic(filter_name: &str) -> TopicFilter {
    TopicFilter::new(filter_name).expect("Invalid topic filter name")
}
