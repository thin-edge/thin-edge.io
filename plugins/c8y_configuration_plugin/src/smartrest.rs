use crate::config::PluginConfig;
use c8y_smartrest::topic::C8yTopic;
use mqtt_channel::Message;

impl PluginConfig {
    pub fn to_message(&self) -> Result<Message, anyhow::Error> {
        let topic = C8yTopic::SmartRestResponse.to_topic()?;
        Ok(Message::new(&topic, self.to_smartrest_payload()))
    }

    // 119,typeA,typeB,...
    fn to_smartrest_payload(&self) -> String {
        let config_types = self
            .files
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!("119,{config_types}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(
        PluginConfig {
            files: vec!["typeA".to_string()]
        },
        "119,typeA".to_string()
        ;"single file"
    )]
    #[test_case(
        PluginConfig {
        files: vec!["typeA".to_string(), "typeB".to_string(), "typeC".to_string()]
        },
        "119,typeA,typeB,typeC".to_string()
        ;"multiple files"
    )]
    fn get_smartrest(input: PluginConfig, expected_output: String) {
        let output = input.to_smartrest_payload();
        assert_eq!(output, expected_output);
    }
}
