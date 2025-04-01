use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

wit_bindgen::generate!({
    world: "tedge",
    path: "../../../wit/world.wit",
});

pub struct Component;

impl Component {
    fn format_timestamp(timestamp: Datetime) -> Result<String, String> {
        OffsetDateTime::from_unix_timestamp(timestamp.seconds as i64)
            .map_err(|e| e.to_string())?
            .replace_nanosecond(timestamp.nanoseconds)
            .map_err(|e| e.to_string())?
            .format(&Rfc3339)
            .map_err(|e| e.to_string())
    }
}

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

        let now_utc = Self::format_timestamp(timestamp).map_err(|err| {
            FilterError::IncorrectSetting(format!("failed to format current timestamp: {}", err))
        })?;

        json.insert("time".to_string(), Value::String(now_utc));

        let updated_message = Message {
            payload: Value::Object(json).to_string(),
            ..message
        };

        Ok(vec![updated_message])
    }

    /// Not configurable
    fn update_config(_config: Message) -> Result<(), FilterError> {
        Ok(())
    }

    /// Stateless
    fn tick(_timestamp: Datetime) -> Result<Vec<Message>, FilterError> {
        Ok(vec![])
    }
}

export!(Component);
