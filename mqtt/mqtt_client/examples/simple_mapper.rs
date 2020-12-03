use mqtt_client::{Client, Message, Topic};

#[tokio::main]
pub async fn main() -> Result<(), mqtt_client::Error> {
    let name = "c8y_mapper";
    let in_topic = Topic::new("tedge/measurements");
    let out_topic = Topic::new("c8y/s/us");
    let err_topic = Topic::new("tegde/errors");

    let mqtt = Client::connect(name).await?;
    let mut errors = mqtt.subscribe_errors();
    tokio::spawn(async move {
        while let Some(error) = errors.next().await {
            eprintln!("ERROR: {}", error);
        }
    });

    let mut messages = mqtt.subscribe(&in_topic).await?;
    while let Some(message) = messages.next().await {
        match translate(&message.payload) {
            Ok(translation) => mqtt.publish(Message::new(&out_topic, translation)).await?,
            Err(error) => mqtt.publish(Message::new(&err_topic, error)).await?,
        }
    }

    Ok(())
}

use json::JsonValue;
const C8Y_TPL_TEMPERATURE: &str = "211";

/// Naive mapper which extracts the temperature field from a ThinEdge Json value.
///
/// `{ "temperature": 12.4 }` is translated into `"12.4"`
fn translate(input: &Vec<u8>) -> Result<Vec<u8>, String> {
    let input = std::str::from_utf8(input).map_err(|err| format!("ERROR: {}", err))?;
    let json = json::parse(input).map_err(|err| format!("ERROR: {}", err))?;
    match json {
        JsonValue::Object(obj) => {
            for (k, v) in obj.iter() {
                if k != "temperature" {
                    return Err(format!("ERROR: unknown measurement '{}'", k));
                }
                match v {
                    JsonValue::Number(num) => {
                        let value: f64 = (*num).into();
                        if value == 0.0 || value.is_normal() {
                            return Ok(format!("{},{}", C8Y_TPL_TEMPERATURE, value).into_bytes());
                        } else {
                            return Err(format!("ERROR: value out of range '{}'", v));
                        }
                    }
                    _ => return Err(format!("ERROR: expect a number '{}'", v)),
                }
            }
            Err(String::from("ERROR: empty measurement"))
        }
        _ => return Err(String::from("ERROR: expect a JSON object")),
    }
}
