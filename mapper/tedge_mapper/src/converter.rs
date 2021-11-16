use mqtt_client::{Message, TopicFilter, Topic};

#[derive(Debug)]
pub struct MapperConfig {
    pub in_topic_filter: TopicFilter,
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

pub trait Converter: Send + Sync {
    type Error;

    fn convert(
        &mut self,
        input: &Message,
    ) -> Result<Vec<Message>, Self::Error>;
}
