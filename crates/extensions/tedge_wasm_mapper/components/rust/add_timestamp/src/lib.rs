use serde_json::Value;

wit_bindgen::generate!({
    world: "tedge",
    path: "../../../wit/world.wit",
});

pub struct Component;

impl Guest for Component {
    fn process(timestamp: Datetime, message: Message) -> Result<Vec<Message>, FilterError> {
        let Ok(Value::Object(mut json)) = serde_json::from_str(&message.payload) else {
            return Err(FilterError::UnsupportedMessage(
                "Expect JSON input".to_string(),
            ));
        };

        if json.get("time").is_some() {
            return Ok(vec![message]);
        }

        let seconds = timestamp.seconds as f64;
        let milliseconds = (timestamp.nanoseconds / 1_000_000) as f64;
        let time = serde_json::Number::from_f64(seconds + milliseconds / 1000.0).unwrap();

        json.insert("time".to_string(), Value::Number(time));

        let updated_message = Message {
            payload: Value::Object(json).to_string(),
            ..message
        };

        Ok(vec![updated_message])
    }

    fn update_config(_config: Message) -> Result<(), FilterError> {
        Ok(())
    }

    fn tick(_timestamp: Datetime) -> Result<Vec<Message>, FilterError> {
        Ok(vec![])
    }
}

export!(Component);
