use std::time::Duration;

use log;

use json::JsonValue;

use mqtt_client::Client;
use mqtt_client::Message;
use mqtt_client::Topic;
use tokio::task::JoinHandle;

const IN_TOPIC: &str = "tedge/measurements";
const C8Y_TOPIC: &str = "c8y/s/us";
const ERRORS_TOPIC: &str = "tedge/errors";

pub struct Mapper {
    client: mqtt_client::Client,
    in_topic: mqtt_client::Topic,
    out_topic: mqtt_client::Topic,
    err_topic: mqtt_client::Topic,
}

const C8Y_TEMPLATE_TEMPERATURE: &str = "211";

impl Mapper {
    pub fn new(
        client: mqtt_client::Client,
        in_topic: &str,
        out_topic: &str,
        err_topic: &str,
    ) -> Mapper {
        let new_in_topic = match Topic::new(in_topic) {
            Ok(topic) => topic,
            Err(error) => {
                log::error!("{}", error);
                Topic {
                    name: IN_TOPIC.to_string(),
                }
            }
        };

        let new_out_topic = match Topic::new(out_topic) {
            Ok(topic) => topic,
            Err(error) => {
                log::error!("{}", error);
                Topic {
                    name: C8Y_TOPIC.to_string(),
                }
            }
        };

        let new_err_topic = match Topic::new(err_topic) {
            Ok(topic) => topic,
            Err(error) => {
                log::error!("{}", error);
                Topic {
                    name: ERRORS_TOPIC.to_string(),
                }
            }
        };

        Mapper {
            client: client,
            in_topic: new_in_topic,
            out_topic: new_out_topic,
            err_topic: new_err_topic,
        }
    }

    fn subsribe_errors(&self) -> JoinHandle<()> {
        let mut errors = self.client.subscribe_errors();
        tokio::spawn(async move {
            while let Some(error) = errors.next().await {
                log::error!("{}", error);
            }
        })
    }

    pub async fn subscribe_messages(&self) -> Result<(), mqtt_client::Error> {
        self.subsribe_errors();
        let mut messages = self.client.subscribe(self.in_topic.filter()).await?;
        Ok(while let Some(message) = messages.next().await {
            log::debug!("Mapping {:?}", message);
            match Mapper::map(&message.payload) {
                Ok(mapped) => {
                    self.client
                        .publish(Message::new(&self.out_topic, mapped))
                        .await?
                }
                Err(error) => {
                    log::debug!("Translation error: {}", error);
                    self.client
                        .publish(Message::new(&self.err_topic, error))
                        .await?
                }
            }
        })
    }

    /// mapper which extracts the temperature field from a ThinEdge Json value.
    ///
    ///
    pub fn map(input: &Vec<u8>) -> Result<Vec<u8>, String> {
        let input = std::str::from_utf8(input).map_err(|err| format!("ERROR: {}", err))?;
        let json = json::parse(input).map_err(|err| format!("ERROR: {}", err))?;
        match json {
            JsonValue::Object(obj) => {
                for (key, value) in obj.iter() {
                    match key {
                        "temperature" => {}
                        _ => {
                            return Err(format!("ERROR: only supported temperature, not '{}'", key))
                        }
                    };

                    match value {
                        JsonValue::Number(num) => {
                            let value: f64 = (*num).into();
                            if value == 0.0 || value.is_normal() {
                                return Ok(
                                    format!("{},{}", C8Y_TEMPLATE_TEMPERATURE, value).into_bytes()
                                );
                            } else {
                                return Err(format!("ERROR: value out of range '{}'", value));
                            }
                        }
                        _ => return Err(format!("ERROR: expected a number, not '{}'", value)),
                    }
                }
                return Err(String::from("ERROR: empty measurement"));
            }
            _ => return Err(format!("ERROR: expected a JSON object, not {}", json)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_mapper_convert_number() {
        let input = String::into_bytes("{\"temperature\": 124}".to_owned());
        let output = Result::Ok(String::into_bytes("211,124".to_owned()));
        assert_eq!(Mapper::map(&input), output);
    }
    #[test]
    fn test_mapper_convert_string() {
        let input = String::into_bytes("{\"temperature\": \"test\"}".to_owned());
        let output = Result::Err("ERROR: expected a number, not \'test\'".to_owned());
        assert_eq!(Mapper::map(&input), output);
    }
}
