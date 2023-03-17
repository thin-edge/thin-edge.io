//use crate::c8y::dynamic_discovery::DiscoverOp;
use async_trait::async_trait;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use std::fmt::Display;
use tracing::error;

#[derive(Debug)]
pub struct MapperConfig {
    pub in_topic_filter: TopicFilter,
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

#[async_trait]
pub trait Converter: Send + Sync {
    type Error: Display;

    fn get_mapper_config(&self) -> &MapperConfig;

    fn get_in_topic_filter(&self) -> &TopicFilter {
        &self.get_mapper_config().in_topic_filter
    }

    async fn try_convert(&mut self, input: &Message) -> Result<Vec<Message>, Self::Error>;

    async fn convert(&mut self, input: &Message) -> Vec<Message> {
        let messages_or_err = self.try_convert(input).await;
        self.wrap_errors(messages_or_err)
    }

    fn wrap_errors(&self, messages_or_err: Result<Vec<Message>, Self::Error>) -> Vec<Message> {
        messages_or_err.unwrap_or_else(|error| vec![self.new_error_message(error)])
    }

    fn wrap_error(&self, message_or_err: Result<Message, Self::Error>) -> Message {
        message_or_err.unwrap_or_else(|error| self.new_error_message(error))
    }

    fn new_error_message(&self, error: Self::Error) -> Message {
        error!("Mapping error: {}", error);
        Message::new(&self.get_mapper_config().errors_topic, error.to_string())
    }

    fn try_init_messages(&mut self) -> Result<Vec<Message>, Self::Error> {
        Ok(vec![])
    }

    /// This function will be the first method that's called on the converter after it's instantiated.
    /// Return any initialization messages that must be processed before the converter starts converting regular messages.
    fn init_messages(&mut self) -> Vec<Message> {
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

    /// This function will be the called after a brief period(sync window) after the converter starts converting messages.
    /// This gives the converter an opportunity to process the messages received during the sync window and
    /// produce any additional messages as "sync messages" as a result of this processing.
    /// These sync messages will be processed by the mapper right after the sync window before it starts converting further messages.
    /// Typically used to do some processing on all messages received on mapper startup and derive additional messages out of those.
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
