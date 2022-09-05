use std::collections::{hash_map::Entry, HashMap};

use c8y_smartrest::alarm;
use mqtt_channel::{Message, Topic};
use thin_edge_json::alarm::ThinEdgeAlarm;

use crate::core::error::ConversionError;

const TEDGE_ALARMS_TOPIC: &str = "tedge/alarms/";
const INTERNAL_ALARMS_TOPIC: &str = "c8y-internal/alarms/";
const SMARTREST_PUBLISH_TOPIC: &str = "c8y/s/us";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AlarmConverter {
    Syncing {
        pending_alarms_map: HashMap<String, Message>,
        old_alarms_map: HashMap<String, Message>,
    },
    Synced,
}

impl AlarmConverter {
    pub(crate) fn new() -> Self {
        AlarmConverter::Syncing {
            old_alarms_map: HashMap::new(),
            pending_alarms_map: HashMap::new(),
        }
    }

    pub(crate) fn try_convert_alarm(
        &mut self,
        input_message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        let mut output_messages: Vec<Message> = Vec::new();
        match self {
            Self::Syncing {
                pending_alarms_map,
                old_alarms_map: _,
            } => {
                let alarm_id = input_message
                    .topic
                    .name
                    .strip_prefix(TEDGE_ALARMS_TOPIC)
                    .expect("Expected tedge/alarms prefix")
                    .to_string();
                pending_alarms_map.insert(alarm_id, input_message.clone());
            }
            Self::Synced => {
                //Regular conversion phase
                let tedge_alarm = ThinEdgeAlarm::try_from(
                    input_message.topic.name.as_str(),
                    input_message.payload_str()?,
                )?;
                let smartrest_alarm = alarm::serialize_alarm(tedge_alarm)?;
                let c8y_alarm_topic = Topic::new_unchecked(
                    self.get_c8y_alarm_topic(input_message.topic.name.as_str())?
                        .as_str(),
                );
                output_messages.push(Message::new(&c8y_alarm_topic, smartrest_alarm));

                // Persist a copy of the alarm to an internal topic for reconciliation on next restart
                let alarm_id = input_message
                    .topic
                    .name
                    .strip_prefix(TEDGE_ALARMS_TOPIC)
                    .expect("Expected tedge/alarms prefix")
                    .to_string();
                let topic =
                    Topic::new_unchecked(format!("{INTERNAL_ALARMS_TOPIC}{alarm_id}").as_str());
                let alarm_copy =
                    Message::new(&topic, input_message.payload_bytes().to_owned()).with_retain();
                output_messages.push(alarm_copy);
            }
        }
        Ok(output_messages)
    }

    pub(crate) fn get_c8y_alarm_topic(&self, topic: &str) -> Result<String, ConversionError> {
        let topic_split: Vec<&str> = topic.split('/').collect();
        if topic_split.len() == 4 {
            Ok(SMARTREST_PUBLISH_TOPIC.to_string())
        } else if topic_split.len() == 5 {
            Ok(format!("{SMARTREST_PUBLISH_TOPIC}/{}", topic_split[4]))
        } else {
            Err(ConversionError::UnsupportedTopic(topic.to_string()))
        }
    }

    pub(crate) fn process_internal_alarm(&mut self, input: &Message) {
        match self {
            Self::Syncing {
                pending_alarms_map: _,
                old_alarms_map,
            } => {
                let alarm_id = input
                    .topic
                    .name
                    .strip_prefix(INTERNAL_ALARMS_TOPIC)
                    .expect("Expected c8y-internal/alarms prefix")
                    .to_string();
                old_alarms_map.insert(alarm_id, input.clone());
            }
            Self::Synced => {
                // Ignore
            }
        }
    }

    /// Detect and sync any alarms that were raised/cleared while this mapper process was not running.
    /// For this syncing logic, converter maintains an internal journal of all the alarms processed by this mapper,
    /// which is compared against all the live alarms seen by the mapper on every startup.
    ///
    /// All the live alarms are received from tedge/alarms topic on startup.
    /// Similarly, all the previously processed alarms are received from c8y-internal/alarms topic.
    /// Sync detects the difference between these two sets, which are the missed messages.
    ///
    /// An alarm that is present in c8y-internal/alarms, but not in tedge/alarms topic
    /// is assumed to have been cleared while the mapper process was down.
    /// Similarly, an alarm that is present in tedge/alarms, but not in c8y-internal/alarms topic
    /// is one that was raised while the mapper process was down.
    /// An alarm present in both, if their payload is the same, is one that was already processed before the restart
    /// and hence can be ignored during sync.
    pub(crate) fn sync(&mut self) -> Vec<Message> {
        let mut sync_messages: Vec<Message> = Vec::new();

        match self {
            Self::Syncing {
                pending_alarms_map,
                old_alarms_map,
            } => {
                // Compare the differences between alarms in tedge/alarms topic to the ones in c8y-internal/alarms topic
                old_alarms_map.drain().for_each(|(alarm_id, old_message)| {
                    match pending_alarms_map.entry(alarm_id.clone()) {
                        // If an alarm that is present in c8y-internal/alarms topic is not present in tedge/alarms topic,
                        // it is assumed to have been cleared while the mapper process was down
                        Entry::Vacant(_) => {
                            let topic = Topic::new_unchecked(
                                format!("{TEDGE_ALARMS_TOPIC}{alarm_id}").as_str(),
                            );
                            let message = Message::new(&topic, vec![]).with_retain();
                            // Recreate the clear alarm message and add it to the pending alarms list to be processed later
                            sync_messages.push(message);
                        }

                        // If the payload of a message received from tedge/alarms is same as one received from c8y-internal/alarms,
                        // it is assumed to be one that was already processed earlier and hence removed from the pending alarms list.
                        Entry::Occupied(entry) => {
                            if entry.get().payload_bytes() == old_message.payload_bytes() {
                                entry.remove();
                            }
                        }
                    }
                });

                pending_alarms_map
                    .drain()
                    .for_each(|(_key, message)| sync_messages.push(message));
            }
            Self::Synced => {
                // Ignore
            }
        }
        sync_messages
    }
}
