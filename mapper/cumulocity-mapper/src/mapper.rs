use core::fmt;
use open_json::MeasurementRecord;
use rumqttc::Event::Incoming;
use rumqttc::Packet::Publish;
use rumqttc::Packet::Disconnect;
use rumqttc::QoS;
use tokio_compat_02::FutureExt;

/// Convert a measurement record into a sequence of SmartRest messages
///
/// ```
/// use mapper;
/// use open_json::MeasurementRecord;
///
/// let input = r#"{
///     "temperature": 23,
///     "battery": 99
/// }"#;
///
/// let record = MeasurementRecord::from_json(input).unwrap();
/// let smart_rest = mapper::into_smart_rest(&record).unwrap();
///
/// assert_eq!(smart_rest, vec![
///     "211,23".to_string(),
///     "212,99".to_string(),
/// ]);
/// ```
pub fn into_smart_rest(record: &MeasurementRecord) -> Result<Vec<String>, Error> {
    let mut messages = Vec::new();
    for (k, v) in record.measurements().iter() {
        if k == "temperature" {
            messages.push(format!("211,{}", v));
        } else if k == "battery" {
            messages.push(format!("212,{}", v));
        } else {
            return Err(Error::UnknownTemplate(k.clone()));
        }
    }
    Ok(messages)
}

/// The configuration of mapper
///
/// ```
/// let default_config = mapper::Configuration::default();
///
/// assert_eq!(default_config.name, "c8y-mapper");
/// assert_eq!(default_config.in_topic, "tedge/measurements");
/// assert_eq!(default_config.out_topic, "c8y/s/us");
/// assert_eq!(default_config.err_topic, "tegde/errors");
/// ```
pub struct Configuration {
    pub name: String,
    pub in_topic: String,
    pub out_topic: String,
    pub err_topic: String,
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            name: "c8y-mapper".to_string(),
            in_topic: "tedge/measurements".to_string(),
            out_topic: "c8y/s/us".to_string(),
            err_topic: "tegde/errors".to_string(),
        }
    }
}

/// A mapper event loop,
/// - listening for ThinEdge measurement messages,
/// - translating these messages into SmartRest2 messages,
/// - forwarding the translated messages to Cumulocity.
///
/// ```no_run
/// let config = mapper::Configuration::default();
/// let mut mapper = mapper::EventLoop::new(config);
/// mapper.run();
/// ```
pub struct EventLoop {
    conf: Configuration,
    mqtt_client: rumqttc::AsyncClient,
    eventloop: rumqttc::EventLoop,
}

impl EventLoop {
    /// Create a new mapper event-loop for a configuration
    pub fn new(conf: Configuration) -> EventLoop {
        let mut mqtt_options = rumqttc::MqttOptions::new(&conf.name, "localhost", 1883);
        mqtt_options.set_clean_session(false);
        let (mqtt_client, eventloop) = rumqttc::AsyncClient::new(mqtt_options, 10);

        EventLoop {
            conf,
            mqtt_client,
            eventloop,
        }
    }

    /// Run the mapper event loop:
    /// - listening for Open Json measurement records,
    /// - translating these records into SmartRest2 messages,
    /// - forwarding these messages to Cumulocity.
    pub async fn run(&mut self) -> Result<(), Error> {
        let conf = &self.conf;
        let eventloop = &mut self.eventloop;
        let mqtt_client = &mut self.mqtt_client;

        let qos = QoS::ExactlyOnce;

        mqtt_client
            .subscribe(&conf.in_topic, qos)
            .compat()
            .await
            .map_err(|err| Error::MqttSubFail(format!("{}", err)))?;

        println!("Translating: {} -> {}", &conf.in_topic, &conf.out_topic);
        loop {
            match eventloop.poll().compat().await {
                Ok(Incoming(Disconnect)) => break,

                Ok(Incoming(Publish(input))) if &input.topic == &conf.in_topic => {
                    let record = match MeasurementRecord::from_bytes(&input.payload) {
                        Ok(rec) => rec,
                        Err(err) => {
                            let err_msg = format!("{}", Error::BadOpenJson(err));
                            Self::publish(mqtt_client, &conf.err_topic, qos, err_msg).await;
                            continue;
                        }
                    };
                    println!("    {} ->", record);
                    let messages = match into_smart_rest(&record) {
                        Ok(messages) => messages,
                        Err(err) => {
                            let err_msg = format!("{}", err);
                            Self::publish(mqtt_client, &conf.err_topic, qos, err_msg).await;
                            continue;
                        }
                    };
                    for msg in messages.into_iter() {
                        println!("    -> {}", msg);
                        Self::publish(mqtt_client, &conf.out_topic, qos, msg).await;
                    }
                }

                Err(err) => {
                    Self::log(Error::MqttSubFail(format!("{}", err)));
                    continue;

                }

                _ => (),
            }
        }
        Ok(())
    }

    async fn publish(mqtt_client: &mut rumqttc::AsyncClient, topic: &String, qos: QoS, msg: String) {
        if let Some(err) = mqtt_client.publish(topic, qos, false, msg).compat().await.err() {
            Self::log(Error::MqttPubFail(format!("{}", err)));
        }
    }

    fn log(err: Error) {
        eprintln!("ERROR: {}", err);
    }
}

/// Translation errors
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    BadOpenJson(open_json::Error),
    UnknownTemplate(String),
    MqttPubFail(String),
    MqttSubFail(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::BadOpenJson(ref err) => write!(f, "Open Json error: {}", err),
            Error::UnknownTemplate(ref t) => write!(f, "Unknown template: '{}'", t),
            Error::MqttPubFail(ref err) => write!(f, "MQTT error publishing: {}", err),
            Error::MqttSubFail(ref err) => write!(f, "MQTT error subscribing: {}", err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_temperature() {
        let input = r#"{"temperature": 23}"#;
        let expected = vec!["211,23".into()];
        let record = MeasurementRecord::from_json(input).unwrap();
        assert_eq!(Ok(expected), into_smart_rest(&record))
    }

    #[test]
    fn map_battery() {
        let input = r#"{"battery": 99}"#;
        let expected = vec!["212,99".into()];
        let record = MeasurementRecord::from_json(input).unwrap();
        assert_eq!(Ok(expected), into_smart_rest(&record))
    }

    #[test]
    fn map_record() {
        let input = r#"{"temperature": 23, "battery": 99}"#;
        let expected = vec!["211,23".into(), "212,99".into()];
        let record = MeasurementRecord::from_json(input).unwrap();
        assert_eq!(Ok(expected), into_smart_rest(&record))
    }

    #[test]
    fn unknown_template() {
        let input = r#"{"pressure": 20}"#;
        let record = MeasurementRecord::from_json(input).unwrap();
        assert_eq!(
            Err(Error::UnknownTemplate("pressure".into())),
            into_smart_rest(&record)
        )
    }
}
