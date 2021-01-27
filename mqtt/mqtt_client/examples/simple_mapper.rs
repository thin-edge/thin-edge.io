use json::JsonValue;
use log::{debug, error, info};
use mqtt_client::{Client, Config, Message, Topic};

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let name = "c8y_mapper";
    let in_topic = Topic::new("tedge/measurements")?;
    let out_topic = Topic::new("c8y/s/us")?;
    let err_topic = Topic::new("tedge/errors")?;

    env_logger::init();

    info!("Mapping ThinEdge messages");
    let mqtt = Client::connect(name, &Config::default()).await?;
    let mut errors = mqtt.subscribe_errors();
    tokio::spawn(async move {
        while let Some(error) = errors.next().await {
            error!("{}", error);
        }
    });

    let mut messages = mqtt.subscribe(in_topic.filter()).await?;
    while let Some(message) = messages.next().await {
        debug!("Mapping {:?}", message);
        match translate(&message.payload) {
            Ok(translation) => {
                let _ = mqtt.publish(Message::new(&out_topic, translation)).await?;
            }
            Err(error) => {
                debug!("Translation error: {}", error);
                let _ = mqtt.publish(Message::new(&err_topic, error)).await?;
            }
        }
    }

    Ok(())
}

const C8Y_TEMPLATE_TEMPERATURE: &str = "211";

/// Naive mapper which extracts the temperature field from a ThinEdge Json value.
///
/// `{ "temperature": 12.4 }` is translated into `"211,12.4"`
fn translate(raw_input: &Vec<u8>) -> Result<Vec<u8>, String> {
    let input = std::str::from_utf8(raw_input).map_err(|err| format!("ERROR: {}", err))?;
    let json = json::parse(input).map_err(|err| format!("ERROR: {}", err))?;
    match json {
        JsonValue::Object(obj) => {
            for (k, v) in obj.iter() {
                if k != "temperature" {
                    return Err(format!("ERROR: unknown measurement type '{}'", k));
                }
                match v {
                    JsonValue::Number(num) => {
                        let value: f64 = (*num).into();
                        if value == 0.0 || value.is_normal() {
                            return Ok(
                                format!("{},{}", C8Y_TEMPLATE_TEMPERATURE, value).into_bytes()
                            );
                        } else {
                            return Err(format!("ERROR: value out of range '{}'", v));
                        }
                    }
                    _ => return Err(format!("ERROR: expected a number, not '{}'", v)),
                }
            }
            Err(String::from("ERROR: empty measurement"))
        }
        _ => return Err(format!("ERROR: expected a JSON object, not {}", json)),
    }
}
