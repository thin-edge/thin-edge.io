wit_bindgen::generate!({
    world: "tedge",
    path: "../../../wit/filter.wit",
});

use crate::exports::tedge::filter::filtering::Datetime;
use crate::exports::tedge::filter::filtering::Filter;
use crate::exports::tedge::filter::filtering::FilterError;
use crate::exports::tedge::filter::filtering::Guest;
use crate::exports::tedge::filter::filtering::GuestFilter;
use crate::exports::tedge::filter::filtering::Message;
use serde::Deserialize;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub struct Component;

export!(Component);

impl Guest for Component {
    type Filter = GroupByTimestamp;

    fn new_filter(config: Message) -> Filter {
        Filter::new(GroupByTimestamp::new(config))
    }
}

/// Group thin-edge measurements occurring the same time-window
///
/// This POC ignores duplicates and simply takes the latest value
pub struct GroupByTimestamp {
    time_window_secs: u64,
    grouped_messages: RefCell<HashMap<u64, HashMap<Topic, Measurements>>>,
}

type Topic = String;

impl GuestFilter for GroupByTimestamp {
    fn new(_config: Message) -> Self {
        GroupByTimestamp {
            time_window_secs: 60,
            grouped_messages: RefCell::new(HashMap::new()),
        }
    }

    fn process(
        &self,
        _ingestion_timestamp: Datetime,
        message: Message,
    ) -> Result<Vec<Message>, FilterError> {
        let Ok(measurements) = serde_json::from_str::<Measurements>(&message.payload) else {
            return Err(FilterError::UnsupportedMessage(
                "Expect thin-edge measurements".to_string(),
            ));
        };

        let Some(measurement_time) = &measurements.time else {
            return Err(FilterError::UnsupportedMessage(
                "Missing timestamp".to_string(),
            ));
        };

        let unix_timestamp = measurement_time
            .unix_timestamp()
            .map_err(FilterError::UnsupportedMessage)?;

        let measurement_topic = message.topic.clone();
        let time_window_secs = (unix_timestamp / self.time_window_secs) * self.time_window_secs;
        self.grouped_messages
            .borrow_mut()
            .entry(time_window_secs)
            .or_default()
            .entry(measurement_topic)
            .or_default()
            .merge(measurements);

        Ok(vec![])
    }

    fn update_config(&self, _config: Message) -> Result<(), FilterError> {
        Ok(())
    }

    fn tick(&self, timestamp: Datetime) -> Result<Vec<Message>, FilterError> {
        let time_window_secs = (timestamp.seconds / self.time_window_secs) * self.time_window_secs;
        let mut elapsed = vec![];
        for t in self.grouped_messages.borrow().keys() {
            if *t < time_window_secs {
                elapsed.push(*t);
            }
        }

        let mut all_elapsed_messages = vec![];
        for t in elapsed {
            if let Some(elapsed_groups) = self.grouped_messages.borrow_mut().remove(&t) {
                let mut elapsed_messages = GroupByTimestamp::into_messages(t, elapsed_groups)?;
                all_elapsed_messages.append(&mut elapsed_messages);
            }
        }
        Ok(all_elapsed_messages)
    }
}

impl GroupByTimestamp {
    fn into_messages(
        unix_timestamp: u64,
        group: HashMap<Topic, Measurements>,
    ) -> Result<Vec<Message>, FilterError> {
        let mut elapsed_messages = vec![];
        let ingestion_time = time::OffsetDateTime::from_unix_timestamp(unix_timestamp as i64)
            .map_err(|err| {
                FilterError::IncorrectSetting(format!("Fail to format unix timestamp: {err}"))
            })?
            .format(&Rfc3339)
            .map(Timestamp::Rfc3339)
            .map_err(|err| {
                FilterError::IncorrectSetting(format!("Fail to format timestamp as Rfc3339: {err}"))
            })?;

        for (topic, mut measurements) in group {
            measurements.time = Some(ingestion_time.clone());
            let payload = serde_json::to_string(&measurements).map_err(|err| {
                FilterError::IncorrectSetting(format!("Fail to format measurements as JSON: {err}"))
            })?;
            elapsed_messages.push(Message { topic, payload })
        }
        Ok(elapsed_messages)
    }
}

#[derive(Deserialize, Serialize, Default)]
struct Measurements {
    #[serde(skip_serializing_if = "Option::is_none")]
    time: Option<Timestamp>,

    #[serde(flatten)]
    pub extras: HashMap<String, Measurement>,
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum Measurement {
    Number(serde_json::Number),
    Text(String),
    Group(HashMap<String, serde_json::Number>),
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum Timestamp {
    Rfc3339(String),
    UnixTimestamp(serde_json::Number),
}

impl Measurements {
    /// Merge into this set of measurements a new set of measurements
    ///
    /// Returns all the measurements that cannot be merged, if any
    pub fn merge(&mut self, other: Measurements) -> Option<Measurements> {
        let mut rejected_vals = HashMap::new();
        for (k, new_val) in other.extras {
            match self.extras.get_mut(k.as_str()) {
                None => {
                    self.extras.insert(k, new_val);
                }
                Some(old_val) => {
                    if let Some(rejected_val) = old_val.merge(new_val) {
                        rejected_vals.insert(k, rejected_val);
                    }
                }
            };
        }

        if rejected_vals.is_empty() {
            None
        } else {
            Some(Measurements {
                time: other.time.clone(),
                extras: rejected_vals,
            })
        }
    }
}

impl Measurement {
    /// Merge this measurement with a new one
    ///
    /// Returns the new measurement if it cannot be merged
    pub fn merge(&mut self, other: Measurement) -> Option<Measurement> {
        match (self, other) {
            (Measurement::Number(ref mut left), Measurement::Number(right)) => {
                *left = right;
                None
            }
            (Measurement::Text(ref mut left), Measurement::Text(right)) => {
                *left = right;
                None
            }
            (Measurement::Group(ref mut left), Measurement::Group(right)) => {
                for (k, v) in right {
                    left.insert(k, v);
                }
                None
            }
            (_, other) => Some(other),
        }
    }
}

impl Timestamp {
    pub fn unix_timestamp(&self) -> Result<u64, String> {
        match self {
            Timestamp::Rfc3339(datetime) => OffsetDateTime::parse(datetime, &Rfc3339)
                .map(|dt| dt.unix_timestamp() as u64)
                .map_err(|_| "Expect Rfc3339 timestamp".to_string()),
            Timestamp::UnixTimestamp(number) => number
                .as_f64()
                .map(|t| t.round() as u64)
                .ok_or("Expect unix timestamp".to_string()),
        }
    }
}
