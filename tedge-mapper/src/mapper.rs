use c8y_json_translator::CumulocityJson;

use log;

use mqtt_client;

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

impl Mapper {
    pub fn new(
        client: mqtt_client::Client,
        in_topic: &str,
        out_topic: &str,
        err_topic: &str,
    ) -> Mapper {
        let new_in_topic = match mqtt_client::Topic::new(in_topic) {
            Ok(topic) => topic,
            Err(error) => {
                log::error!("{}", error);
                mqtt_client::Topic {
                    name: IN_TOPIC.to_string(),
                }
            }
        };

        let new_out_topic = match mqtt_client::Topic::new(out_topic) {
            Ok(topic) => topic,
            Err(error) => {
                log::error!("{}", error);
                mqtt_client::Topic {
                    name: C8Y_TOPIC.to_string(),
                }
            }
        };

        let new_err_topic = match mqtt_client::Topic::new(err_topic) {
            Ok(topic) => topic,
            Err(error) => {
                log::error!("{}", error);
                mqtt_client::Topic {
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
        while let Some(message) = messages.next().await {
            log::debug!("Mapping {:?}", message);
            match Mapper::map(&message.payload) {
                Ok(mapped) => {
                    self.client
                        .publish(mqtt_client::Message::new(&self.out_topic, mapped))
                        .await?
                }
                Err(error) => {
                    log::debug!("Mapping error: {}", error);
                    self.client
                        .publish(mqtt_client::Message::new(
                            &self.err_topic,
                            error.to_string(),
                        ))
                        .await?
                }
            }
        }
        Ok(())
    }

    fn map(input: &Vec<u8>) -> Result<Vec<u8>, c8y_json_translator::ThinEdgeJsonError> {
        CumulocityJson::from_thin_edge_json(&input[..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_mapper_convert_number_with_time() {
        let input = String::into_bytes(
            "{\"time\": \"2021-01-13T11:00:47.236416800+00:00\",\"temperature\": 124}".to_owned(),
        );
        let output = String::into_bytes("{\"type\":\"ThinEdgeMeasurement\",\"time\":\"2021-01-13T11:00:47.236416800+00:00\",\"temperature\":{\"temperature\":{\"value\":124}}}".to_owned());
        let result = Mapper::map(&input);
        match result {
            Ok(result) => assert_eq!(result, output),
            _ => {}
        }
    }

    #[test]
    fn test_mapper_convert_number_without_time() {
        let input = String::into_bytes("{\"temperature\": 124}".to_owned());
        let output = String::from("\"temperature\":{\"temperature\":{\"value\":124}}}");
        let result = Mapper::map(&input);
        match result {
            Ok(result) => assert!(String::from_utf8(result).unwrap().contains(&output)),
            _ => {}
        }
    }

    #[test]
    fn test_mapper_convert_string() {
        let input = String::into_bytes("{\"temperature\": \"test\"}".to_owned());
        let result = Mapper::map(&input);
        match result {
            Err(e) => {
                assert_eq!("InvalidThinEdgeJson temperature", e.to_string());
            }
            _ => {}
        }
    }
}
