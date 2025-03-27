use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use time::OffsetDateTime;

/// A chain of transformation of MQTT messages
pub struct Pipeline {
    /// The source topics
    pub input_topics: TopicFilter,

    /// Transformation stages to apply in order to the messages
    pub stages: Vec<Stage>,
}

/// A message transformation stage
pub struct Stage {
    pub filter: Box<dyn Filter>,
    pub config_topics: TopicFilter,
}

/// A filter process a stream of messages, producing a stream of transformed messages
///
/// Filters are chained along pipelines, consuming MQTT messages as input
/// and producing MQTT messages as output.
///
/// The behavior of a filter can be time related and
///
/// Filters are dynamically configured. New partial configuration updates are sent overtime,
/// giving the opportunity for a filter to adapt its behavior.
pub trait Filter: 'static + Send + Sync {
    /// Process a single message; producing zero, one or more transformed messages
    fn process(
        &mut self,
        timestamp: OffsetDateTime,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, FilterError>;

    /// Update the filter configuration
    fn update_config(&mut self, config: &MqttMessage) -> Result<(), FilterError>;

    /// Close the current time-window; producing zero, one or more accumulated messages
    fn tick(&mut self, timestamp: OffsetDateTime) -> Result<Vec<MqttMessage>, FilterError>;
}

#[derive(thiserror::Error, Debug)]
pub enum FilterError {
    #[error("Input message cannot be processed: {0}")]
    UnsupportedMessage(String),

    #[error("No messages can be processed due to an incorrect setting: {0}")]
    IncorrectSetting(String),
}

impl Pipeline {
    pub fn topics(&self) -> TopicFilter {
        let mut topics = self.input_topics.clone();
        for stage in self.stages.iter() {
            topics.add_all(stage.config_topics.clone())
        }
        topics
    }

    pub fn update_config(&mut self, message: &MqttMessage) -> Result<(), FilterError> {
        for stage in self.stages.iter_mut() {
            if stage.config_topics.accept(message) {
                stage.filter.update_config(message)?
            }
        }
        Ok(())
    }

    pub fn process(
        &mut self,
        timestamp: OffsetDateTime,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, FilterError> {
        self.update_config(message)?;
        if !self.input_topics.accept(message) {
            return Ok(vec![]);
        }

        let mut messages = vec![message.clone()];
        for stage in self.stages.iter_mut() {
            let mut transformed_messages = vec![];
            for filter_output in messages
                .iter()
                .map(|message| stage.filter.process(timestamp, message))
            {
                transformed_messages.extend(filter_output?);
            }
            messages = transformed_messages;
        }
        Ok(messages)
    }

    pub fn tick(&mut self, timestamp: OffsetDateTime) -> Result<Vec<MqttMessage>, FilterError> {
        let mut messages = vec![];
        for stage in self.stages.iter_mut() {
            // Process first the messages triggered upstream by the tick
            let mut transformed_messages = vec![];
            for filter_output in messages
                .iter()
                .map(|message| stage.filter.process(timestamp, message))
            {
                transformed_messages.extend(filter_output?);
            }

            // Only then process the tick
            transformed_messages.extend(stage.filter.tick(timestamp)?);

            // Iterate with all the messages collected at this stage
            messages = transformed_messages;
        }
        Ok(messages)
    }
}
