use mqtt_client::{Message, TopicFilter, Topic};

#[derive(Debug)]
pub struct MapperConfig {
    pub in_topic_filter: TopicFilter,
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

pub trait Converter: Send + Sync {
    type Error;

    fn get_mapper_config(&self) -> &MapperConfig;

    fn get_in_topic_filter(&self) -> &TopicFilter {
        &self.get_mapper_config().in_topic_filter
    }

    fn convert(
        &mut self,
        input: &Message,
    ) -> Result<Vec<Message>, Self::Error>;
}

pub fn make_valid_topic_or_panic(topic_name: &str) -> Topic {
    Topic::new(topic_name).expect("Invalid topic name")
}

pub fn make_valid_topic_filter_or_panic(filter_name: &str) -> TopicFilter {
    TopicFilter::new(filter_name).expect("Invalid topic filter name")
}
