use c8y_api::json_c8y::C8yUpdateSoftwareListResponse;
use c8y_api::smartrest;
use tedge_api::CommandStatus;
use tedge_api::SoftwareListCommand;
use tedge_config::SoftwareManagementApiFlag;
use tedge_mqtt_ext::MqttMessage;
use tracing::error;

use crate::error::ConversionError;

use super::EntityTarget;
use super::OperationContext;
use super::OperationResult;

const SOFTWARE_LIST_CHUNK_SIZE: usize = 100;

impl OperationContext {
    pub async fn publish_software_list(
        &self,
        target: &EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<OperationResult, ConversionError> {
        let command = match SoftwareListCommand::try_from_bytes(
            target.topic_id.clone(),
            cmd_id.to_owned(),
            message.payload_bytes(),
        )? {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok(OperationResult::Ignored);
            }
        };

        match command.status() {
            CommandStatus::Successful => {
                // Send a list via HTTP to support backwards compatibility to c8y < 10.14
                if self.software_management_api == SoftwareManagementApiFlag::Legacy {
                    let c8y_software_list: C8yUpdateSoftwareListResponse = (&command).into();
                    self.http_proxy
                        .clone()
                        .send_software_list_http(
                            c8y_software_list,
                            target.external_id.as_ref().to_string(),
                        )
                        .await?;
                    return Ok(OperationResult::Finished { messages: vec![] });
                }

                // Send a list via SmartREST, "advanced software list" feature c8y >= 10.14
                let topic = target.smartrest_publish_topic.clone();
                let payloads = smartrest::smartrest_serializer::get_advanced_software_list_payloads(
                    &command,
                    SOFTWARE_LIST_CHUNK_SIZE,
                );

                let mut messages: Vec<MqttMessage> = Vec::new();
                for payload in payloads {
                    messages.push(MqttMessage::new(&topic, payload))
                }

                Ok(OperationResult::Finished { messages })
            }

            CommandStatus::Failed { reason } => {
                error!("Fail to list installed software packages: {reason}");
                Ok(OperationResult::Finished { messages: vec![] })
            }

            CommandStatus::Init
            | CommandStatus::Scheduled
            | CommandStatus::Executing
            | CommandStatus::Unknown => {
                // C8Y doesn't expect any message to be published
                Ok(OperationResult::Ignored)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::Sender;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;

    use crate::tests::skip_init_messages;
    use crate::tests::spawn_c8y_mapper_actor;
    use crate::tests::spawn_dummy_c8y_http_proxy;
    use crate::tests::TestHandle;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    #[tokio::test]
    async fn mapper_publishes_advanced_software_list() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, http, .. } = test_handle;
        spawn_dummy_c8y_http_proxy(http);

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate software_list request
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/software_list/c8y-mapper-1234"),
            json!({
            "id":"1",
            "status":"successful",
            "currentSoftwareList":[
                {"type":"debian", "modules":[
                    {"name":"a"},
                    {"name":"b","version":"1.0"},
                    {"name":"c","url":"https://foobar.io/c.deb"},
                    {"name":"d","version":"beta","url":"https://foobar.io/d.deb"}
                ]},
                {"type":"apama","modules":[
                    {"name":"m","url":"https://foobar.io/m.epl"}
                ]}
            ]})
            .to_string(),
        ))
        .await
        .expect("Send failed");

        assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "140,a,,debian,,b,1.0,debian,,c,,debian,https://foobar.io/c.deb,d,beta,debian,https://foobar.io/d.deb,m,,apama,https://foobar.io/m.epl"
            )
        ])
        .await;
    }
}
