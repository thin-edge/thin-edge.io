use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;

wit_bindgen::generate!({
    world: "tedge",
    path: "../../../wit/world.wit",
});

export!(Component);

struct Component;

impl Guest for Component {
    /// Transform:
    ///
    /// ```
    /// [te/device/main///m/example] {
    ///     "time": "2020-10-15T05:30:47+00:00",
    ///     "temperature": 25,
    ///     "location": {
    ///         "latitude": 32.54,
    ///         "longitude": -117.67,
    ///         "altitude": 98.6
    ///     },
    ///     "pressure": 98
    /// }
    /// ```
    ///
    /// into
    ///
    /// ```
    /// [c8y/measurement/measurements/create] {
    ///     "time": "2020-10-15T05:30:47Z",
    ///     "type": "example",
    ///     "temperature": {
    ///         "temperature": {
    ///             "value": 25
    ///         }
    ///     },
    ///     "location": {
    ///         "latitude": {
    ///             "value": 32.54
    ///         },
    ///         "longitude": {
    ///             "value": -117.67
    ///         },
    ///         "altitude": {
    ///             "value": 98.6
    ///         }
    ///     },
    ///     "pressure": {
    ///         "pressure": {
    ///             "value": 98
    ///         }
    ///     }
    /// }
    /// ```
    fn process(_timestamp: Datetime, message: Message) -> Result<Vec<Message>, FilterError> {
        let Some(measurement_type) = Self::extract_type(&message.topic) else {
            return Err(FilterError::UnsupportedMessage(
                "Expect a measurement topic".to_string(),
            ));
        };

        let Ok(measurements) = serde_json::from_str::<Measurements>(&message.payload) else {
            return Err(FilterError::UnsupportedMessage(
                "Expect thin-edge measurements".to_string(),
            ));
        };

        let c8y_json = Self::into_c8y_measurements(measurement_type, measurements);

        match serde_json::to_string(&c8y_json) {
            Ok(payload) => {
                let c8y_measurements = Message {
                    topic: "c8y/measurement/measurements/create".to_string(),
                    payload,
                };
                Ok(vec![c8y_measurements])
            }

            Err(err) => Err(FilterError::UnsupportedMessage(format!("{err}"))),
        }
    }
}

#[derive(Deserialize)]
struct Measurements {
    #[serde(skip_serializing_if = "Option::is_none")]
    time: Option<Timestamp>,

    #[serde(flatten)]
    pub extras: HashMap<String, Measurement>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Measurement {
    Number(serde_json::Number),
    Text(String),
    Group(HashMap<String, serde_json::Number>),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Timestamp {
    Rfc3339(String),
    UnixTimestamp(serde_json::Number),
}

impl Component {
    pub fn extract_type(topic: impl AsRef<str>) -> Option<String> {
        match topic.as_ref().split('/').collect::<Vec<&str>>()[..] {
            [_, _, _, _, _, "m", measurement_type, ..] => Some(measurement_type.to_string()),
            _ => None,
        }
    }

    fn into_c8y_measurements(
        measurement_type: String,
        measurements: Measurements,
    ) -> serde_json::Value {
        let mut c8y = serde_json::Map::new();

        c8y.insert(
            "type".to_string(),
            serde_json::Value::String(measurement_type),
        );
        if let Some(time) = measurements.time {
            c8y.insert("time".to_string(), time.to_json());
        }

        for (key, measurement) in measurements.extras {
            let c8y_measurement = Self::into_c8y_measurement(key.clone(), measurement);
            c8y.insert(key.clone(), c8y_measurement);
        }
        c8y.into()
    }

    fn into_c8y_measurement(key: String, measurement: Measurement) -> serde_json::Value {
        match measurement {
            Measurement::Number(n) => json!({key: { "value": n}}),
            Measurement::Text(t) => json!({key: { "value": t}}),
            Measurement::Group(map) => {
                let mut c8y = serde_json::Map::new();
                for (key, value) in map {
                    c8y.insert(key, json!({"value": value}));
                }
                c8y.into()
            }
        }
    }
}

impl Timestamp {
    fn to_json(self) -> serde_json::Value {
        match self {
            Timestamp::Rfc3339(t) => serde_json::Value::String(t),
            Timestamp::UnixTimestamp(t) => serde_json::Value::Number(t),
        }
    }
}
