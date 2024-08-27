use anyhow::Context;
use c8y_api::smartrest;
use c8y_api::smartrest::inventory::set_c8y_profile_target_payload;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::device_profile::DeviceProfileCmd;
use tedge_api::CommandStatus;
use tedge_mqtt_ext::MqttMessage;
use tracing::warn;

use super::EntityTarget;
use super::OperationContext;
use super::OperationError;
use super::OperationOutcome;

impl OperationContext {
    pub async fn handle_device_profile_state_change(
        &self,
        target: &EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<OperationOutcome, OperationError> {
        if !self.capabilities.device_profile {
            warn!("Received a device_profile command, however, device_profile feature is disabled");
            return Ok(OperationOutcome::Ignored);
        }

        let command = match DeviceProfileCmd::try_from_bytes(
            target.topic_id.to_owned(),
            cmd_id.into(),
            message.payload_bytes(),
        )
        .context("Could not parse command as a device profile command")?
        {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok(OperationOutcome::Ignored);
            }
        };

        let sm_topic = &target.smartrest_publish_topic;

        match command.status() {
            CommandStatus::Executing => {
                let c8y_target_profile =
                    MqttMessage::new(sm_topic, set_c8y_profile_target_payload(false)); // Set target profile

                Ok(OperationOutcome::Executing {
                    extra_messages: vec![c8y_target_profile],
                })
            }
            CommandStatus::Successful => {
                let c8y_target_profile =
                    MqttMessage::new(sm_topic, set_c8y_profile_target_payload(true)); // Set the target profile as executed

                let smartrest_set_operation =
                    smartrest::smartrest_serializer::succeed_operation_no_payload(
                        CumulocitySupportedOperations::C8yDeviceProfile,
                    );
                let c8y_notification = MqttMessage::new(sm_topic, smartrest_set_operation);

                Ok(OperationOutcome::Finished {
                    messages: vec![
                        c8y_target_profile,
                        c8y_notification,
                        self.request_software_list(&target.topic_id),
                    ],
                })
            }
            CommandStatus::Failed { reason } => {
                let smartrest_set_operation = smartrest::smartrest_serializer::fail_operation(
                    CumulocitySupportedOperations::C8yDeviceProfile,
                    &reason,
                );

                let c8y_notification = MqttMessage::new(sm_topic, smartrest_set_operation);

                Ok(OperationOutcome::Finished {
                    messages: vec![
                        c8y_notification,
                        self.request_software_list(&target.topic_id),
                    ],
                })
            }
            _ => Ok(OperationOutcome::Ignored),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::skip_init_messages;
    use crate::tests::spawn_c8y_mapper_actor;
    use crate::tests::TestHandle;

    use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
    use serde_json::json;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::MessageReceiver;
    use tedge_actors::Sender;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::test_helpers::assert_received_includes_json;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    #[tokio::test]
    async fn create_device_profile_operation_file_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate device_profile cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/device_profile"),
            "{}",
        ))
        .await
        .expect("Send failed");

        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_DeviceProfile")]).await;

        // Validate if the supported operation file is created
        assert!(ttd.path().join("operations/c8y/c8y_DeviceProfile").exists());
    }

    #[tokio::test]
    async fn create_device_profile_operation_file_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate device_profile cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/device_profile"),
            "{}",
        ))
        .await
        .expect("Send failed");

        // Expect auto-registration message
        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/child1//",
                json!({"@type":"child-device","@id":"test-device:device:child1"}),
            )],
        )
        .await;

        assert_received_contains_str(
            &mut mqtt,
            [
                (
                    "c8y/s/us",
                    "101,test-device:device:child1,child1,thin-edge.io-child",
                ),
                (
                    "c8y/s/us/test-device:device:child1",
                    "114,c8y_DeviceProfile",
                ),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/test-device:device:child1/c8y_DeviceProfile")
            .exists());

        // Duplicate device_profile cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/device_profile"),
            "{}",
        ))
        .await
        .expect("Send failed");

        // Assert that the supported ops message is not duplicated
        assert_eq!(mqtt.recv().await, None);
    }

    #[tokio::test]
    async fn mapper_converts_device_profile_operation_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_DeviceProfile operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "profileName": "test-profile",
                "c8y_DeviceProfile": {
                    "software": [
                        {
                            "softwareType": "apt",
                            "name": "test-software-1",
                            "action": "install",
                            "version": "latest",
                            "url": " "
                        },
                        {
                            "softwareType": "apt",
                            "name": "test-software-2",
                            "action": "install",
                            "version": "latest",
                            "url": " "
                        }
                    ],
                    "configuration": [
                        {
                            "name": "test-software-1",
                            "type": "path/config/test-software-1",
                            "url": "http://www.my.url"
                        }
                    ],
                    "firmware": {
                        "name": "test-firmware",
                        "version": "1.0",
                        "url": "http://www.my.url"
                    }
                },
                "externalSource": {
                    "externalId": "test-device",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/main///cmd/device_profile/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "name": "test-profile",
                    "operations": [
                        {
                            "operation": "firmware_update",
                            "skip": false,
                            "payload": {
                                "name": "test-firmware",
                                "version": "1.0",
                                "remoteUrl": "http://www.my.url"
                            }
                        },
                        {
                            "operation": "software_update",
                            "skip": false,
                            "payload": {
                                "updateList": [
                                    {
                                        "type": "apt",
                                        "modules": [
                                            {
                                                "name": "test-software-1",
                                                "version": "latest",
                                                "action": "install"
                                            },
                                            {
                                                "name": "test-software-2",
                                                "version": "latest",
                                                "action": "install"
                                            }
                                        ]
                                    }
                                ]
                            }
                        },
                        {
                            "operation": "config_update",
                            "skip": false,
                            "payload": {
                                "type": "path/config/test-software-1",
                                "remoteUrl":"http://www.my.url"
                            }
                        }
                    ]
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_converts_device_profile_operation_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // The child device must be registered first
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            r#"{ "@type":"child-device", "@id":"child1" }"#,
        ))
        .await
        .expect("fail to register the child-device");

        mqtt.skip(1).await; // Skip child device registration messages

        // Simulate c8y_DeviceProfile operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "profileName": "test-profile",
                "c8y_DeviceProfile": {
                    "software": [
                        {
                            "softwareType": "apt",
                            "name": "test-software-1",
                            "action": "install",
                            "version": "latest",
                            "url": " "
                        },
                        {
                            "softwareType": "apt",
                            "name": "test-software-2",
                            "action": "install",
                            "version": "latest",
                            "url": " "
                        }
                    ],
                    "configuration": [
                        {
                            "name": "test-software-1",
                            "type": "path/config/test-software-1",
                            "url": "http://www.my.url"
                        }
                    ],
                    "firmware": {
                        "name": "test-firmware",
                        "version": "1.0",
                        "url": "http://www.my.url"
                    }
                },
                "externalSource": {
                    "externalId": "child1",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/child1///cmd/device_profile/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "name": "test-profile",
                    "operations": [
                        {
                            "operation": "firmware_update",
                            "skip": false,
                            "payload": {
                                "name": "test-firmware",
                                "version": "1.0",
                                "remoteUrl": "http://www.my.url"
                            }
                        },
                        {
                            "operation": "software_update",
                            "skip": false,
                            "payload": {
                                "updateList": [
                                    {
                                        "type": "apt",
                                        "modules": [
                                            {
                                                "name": "test-software-1",
                                                "version": "latest",
                                                "action": "install"
                                            },
                                            {
                                                "name": "test-software-2",
                                                "version": "latest",
                                                "action": "install"
                                            }
                                        ]
                                    }
                                ]
                            }
                        },
                        {
                            "operation": "config_update",
                            "skip": false,
                            "payload": {
                                "type": "path/config/test-software-1",
                                "remoteUrl":"http://www.my.url"
                            }
                        }
                    ]
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_converts_device_profile_operation_with_type_in_version() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_DeviceProfile operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "profileName": "test-profile",
                "c8y_DeviceProfile": {
                    "software": [
                        {
                            "name": "test-software-1",
                            "action": "install",
                            "version": "latest::apt",
                            "url": " "
                        },
                        {
                            "name": "test-software-2",
                            "action": "install",
                            "version": "latest::apt",
                            "url": " "
                        }
                    ],
                    "configuration": [
                        {
                            "name": "test-software-1",
                            "type": "path/config/test-software-1",
                            "url": "http://www.my.url"
                        }
                    ],
                    "firmware": {
                        "name": "test-firmware",
                        "version": "1.0",
                        "url": "http://www.my.url"
                    }
                },
                "externalSource": {
                    "externalId": "test-device",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/main///cmd/device_profile/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "name": "test-profile",
                    "operations": [
                        {
                            "operation": "firmware_update",
                            "skip": false,
                            "payload": {
                                "name": "test-firmware",
                                "version": "1.0",
                                "remoteUrl": "http://www.my.url"
                            }
                        },
                        {
                            "operation": "software_update",
                            "skip": false,
                            "payload": {
                                "updateList": [
                                    {
                                        "type": "apt",
                                        "modules": [
                                            {
                                                "name": "test-software-1",
                                                "version": "latest",
                                                "action": "install"
                                            },
                                            {
                                                "name": "test-software-2",
                                                "version": "latest",
                                                "action": "install"
                                            }
                                        ]
                                    }
                                ]
                            }
                        },
                        {
                            "operation": "config_update",
                            "skip": false,
                            "payload": {
                                "type": "path/config/test-software-1",
                                "remoteUrl":"http://www.my.url"
                            }
                        }
                    ]
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_converts_device_profile_operation_with_tenant_url() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_DeviceProfile operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "profileName": "test-profile",
                "c8y_DeviceProfile": {
                    "software": [
                        {
                            "softwareType": "apt",
                            "name": "test-software-1",
                            "action": "install",
                            "version": "latest",
                            "url": "http://test.c8y.io/test/software/123456"
                        },
                        {
                            "softwareType": "apt",
                            "name": "test-software-2",
                            "action": "install",
                            "version": "latest",
                            "url": " "
                        }
                    ],
                    "configuration": [
                        {
                            "name": "test-software-1",
                            "type": "path/config/test-software-1",
                            "url": "http://test.c8y.io/test/config/123456"
                        }
                    ],
                    "firmware": {
                        "name": "test-firmware",
                        "version": "1.0",
                        "url": "http://test.c8y.io/test/firmware/123456"
                    }
                },
                "externalSource": {
                    "externalId": "test-device",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/main///cmd/device_profile/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "name": "test-profile",
                    "operations": [
                        {
                            "operation": "firmware_update",
                            "skip": false,
                            "payload": {
                                "name": "test-firmware",
                                "version": "1.0",
                                "remoteUrl": "http://127.0.0.1:8001/c8y/test/firmware/123456"
                            }
                        },
                        {
                            "operation": "software_update",
                            "skip": false,
                            "payload": {
                                "updateList": [
                                    {
                                        "type": "apt",
                                        "modules": [
                                            {
                                                "name": "test-software-1",
                                                "version": "latest",
                                                "action": "install",
                                                "url": "http://127.0.0.1:8001/c8y/test/software/123456"
                                            },
                                            {
                                                "name": "test-software-2",
                                                "version": "latest",
                                                "action": "install"
                                            }
                                        ]
                                    }
                                ]
                            }
                        },
                        {
                            "operation": "config_update",
                            "skip": false,
                            "payload": {
                                "type": "path/config/test-software-1",
                                "remoteUrl":"http://127.0.0.1:8001/c8y/test/config/123456"
                            }
                        }
                    ]
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_converts_device_profile_operation_with_missing_software_type() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_DeviceProfile operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "profileName": "test-profile",
                "c8y_DeviceProfile": {
                    "software": [
                        {
                            "name": "test-software-1",
                            "action": "install",
                            "version": "latest",
                            "url": " "
                        },
                        {
                            "name": "test-software-2",
                            "action": "install",
                            "version": "latest",
                            "url": " "
                        }
                    ],
                    "configuration": [
                        {
                            "name": "test-software-1",
                            "type": "path/config/test-software-1",
                            "url": "http://www.my.url"
                        }
                    ],
                    "firmware": {
                        "name": "test-firmware",
                        "version": "1.0",
                        "url": "http://www.my.url"
                    }
                },
                "externalSource": {
                    "externalId": "test-device",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/main///cmd/device_profile/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "name": "test-profile",
                    "operations": [
                        {
                            "operation": "firmware_update",
                            "skip": false,
                            "payload": {
                                "name": "test-firmware",
                                "version": "1.0",
                                "remoteUrl": "http://www.my.url"
                            }
                        },
                        {
                            "operation": "software_update",
                            "skip": false,
                            "payload": {
                                "updateList": [
                                    {
                                        "type": "default",
                                        "modules": [
                                            {
                                                "name": "test-software-1",
                                                "version": "latest",
                                                "action": "install"
                                            },
                                            {
                                                "name": "test-software-2",
                                                "version": "latest",
                                                "action": "install"
                                            }
                                        ]
                                    }
                                ]
                            }
                        },
                        {
                            "operation": "config_update",
                            "skip": false,
                            "payload": {
                                "type": "path/config/test-software-1",
                                "remoteUrl":"http://www.my.url"
                            }
                        }
                    ]
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_converts_device_profile_operation_with_missing_firmware() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_DeviceProfile operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "profileName": "test-profile",
                "c8y_DeviceProfile": {
                    "software": [
                        {
                            "softwareType": "apt",
                            "name": "test-software-1",
                            "action": "install",
                            "version": "latest",
                            "url": " "
                        },
                        {
                            "softwareType": "apt",
                            "name": "test-software-2",
                            "action": "install",
                            "version": "latest",
                            "url": " "
                        }
                    ],
                    "configuration": [
                        {
                            "name": "test-software-1",
                            "type": "path/config/test-software-1",
                            "url": "http://www.my.url"
                        }
                    ]
                },
                "externalSource": {
                    "externalId": "test-device",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/main///cmd/device_profile/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "name": "test-profile",
                    "operations": [
                        {
                            "operation": "software_update",
                            "skip": false,
                            "payload": {
                                "updateList": [
                                    {
                                        "type": "apt",
                                        "modules": [
                                            {
                                                "name": "test-software-1",
                                                "version": "latest",
                                                "action": "install"
                                            },
                                            {
                                                "name": "test-software-2",
                                                "version": "latest",
                                                "action": "install"
                                            }
                                        ]
                                    }
                                ]
                            }
                        },
                        {
                            "operation": "config_update",
                            "skip": false,
                            "payload": {
                                "type": "path/config/test-software-1",
                                "remoteUrl":"http://www.my.url"
                            }
                        }
                    ]
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_converts_device_profile_operation_with_missing_software() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_DeviceProfile operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "profileName": "test-profile",
                "c8y_DeviceProfile": {
                    "configuration": [
                        {
                            "name": "test-software-1",
                            "type": "path/config/test-software-1",
                            "url": "http://www.my.url"
                        }
                    ],
                    "firmware": {
                        "name": "test-firmware",
                        "version": "1.0",
                        "url": "http://www.my.url"
                    }
                },
                "externalSource": {
                    "externalId": "test-device",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/main///cmd/device_profile/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "name": "test-profile",
                    "operations": [
                        {
                            "operation": "firmware_update",
                            "skip": false,
                            "payload": {
                                "name": "test-firmware",
                                "version": "1.0",
                                "remoteUrl": "http://www.my.url"
                            }
                        },
                        {
                            "operation": "config_update",
                            "skip": false,
                            "payload": {
                                "type": "path/config/test-software-1",
                                "remoteUrl":"http://www.my.url"
                            }
                        }
                    ]
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_converts_device_profile_operation_with_missing_configuration() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_DeviceProfile operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "profileName": "test-profile",
                "c8y_DeviceProfile": {
                    "software": [
                        {
                            "softwareType": "apt",
                            "name": "test-software-1",
                            "action": "install",
                            "version": "latest",
                            "url": " "
                        },
                        {
                            "softwareType": "apt",
                            "name": "test-software-2",
                            "action": "install",
                            "version": "latest",
                            "url": " "
                        }
                    ],
                    "firmware": {
                        "name": "test-firmware",
                        "version": "1.0",
                        "url": "http://www.my.url"
                    }
                },
                "externalSource": {
                    "externalId": "test-device",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/main///cmd/device_profile/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "name": "test-profile",
                    "operations": [
                        {
                            "operation": "firmware_update",
                            "skip": false,
                            "payload": {
                                "name": "test-firmware",
                                "version": "1.0",
                                "remoteUrl": "http://www.my.url"
                            }
                        },
                        {
                            "operation": "software_update",
                            "skip": false,
                            "payload": {
                                "updateList": [
                                    {
                                        "type": "apt",
                                        "modules": [
                                            {
                                                "name": "test-software-1",
                                                "version": "latest",
                                                "action": "install"
                                            },
                                            {
                                                "name": "test-software-2",
                                                "version": "latest",
                                                "action": "install"
                                            }
                                        ]
                                    }
                                ]
                            }
                        }
                    ]
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_config_update_executing_and_failed_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate config_snapshot command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/device_profile/c8y-mapper-123456"),
            json!({
                "status": "executing",
                "name": "test-profile",
                "operations": [
                    {
                        "operation": "firmware_update",
                        "skip": false,
                        "payload": {
                            "name": "test-firmware",
                            "version": "1.0",
                            "url": "http://www.my.url"
                        }
                    },
                    {
                        "operation": "software_update",
                        "skip": false,
                        "payload": {
                            "updateList": [
                                {
                                    "type": "apt",
                                    "modules": [
                                        {
                                            "name": "test-software-1",
                                            "version": "latest",
                                            "action": "install"
                                        },
                                        {
                                            "name": "test-software-2",
                                            "version": "latest",
                                            "action": "install"
                                        }
                                    ]
                                }
                            ]
                        }
                    },
                    {
                        "operation": "config_update",
                        "skip": false,
                        "payload": {
                            "type": "path/config/test-software-1",
                            "remoteUrl":"http://www.my.url"
                        }
                    }
                ]
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `501` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_DeviceProfile")]).await;

        // Expect `121` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "121,false")]).await;

        // Simulate config_snapshot command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/device_profile/c8y-mapper-123456"),
            json!({
                "status": "failed",
                "reason": "Something went wrong",
                "name": "test-profile",
                "operations": [
                    {
                        "operation": "firmware_update",
                        "skip": false,
                        "payload": {
                            "name": "test-firmware",
                            "version": "1.0",
                            "remoteUrl": "http://www.my.url"
                        }
                    },
                    {
                        "operation": "software_update",
                        "skip": false,
                        "payload": {
                            "updateList": [
                                {
                                    "type": "apt",
                                    "modules": [
                                        {
                                            "name": "test-software-1",
                                            "version": "latest",
                                            "action": "install"
                                        },
                                        {
                                            "name": "test-software-2",
                                            "version": "latest",
                                            "action": "install"
                                        }
                                    ]
                                }
                            ]
                        }
                    },
                    {
                        "operation": "config_update",
                        "skip": false,
                        "payload": {
                            "type": "path/config/test-software-1",
                            "remoteUrl":"http://www.my.url"
                        }
                    }
                ]
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `502` smartrest message on `c8y/s/us`.
        assert_received_contains_str(
            &mut mqtt,
            [("c8y/s/us", "502,c8y_DeviceProfile,Something went wrong")],
        )
        .await;

        // An updated list of software is requested
        assert_received_contains_str(
            &mut mqtt,
            [(
                "te/device/main///cmd/software_list/+",
                r#"{"status":"init"}"#,
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_config_update_executing_and_failed_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // The child device must be registered first
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            r#"{ "@type":"child-device", "@id":"child1" }"#,
        ))
        .await
        .expect("fail to register the child-device");

        mqtt.skip(1).await; // Skip child device registration messages

        // Simulate config_snapshot command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/device_profile/c8y-mapper-123456"),
            json!({
                "status": "executing",
                "name": "test-profile",
                "operations": [
                    {
                        "operation": "firmware_update",
                        "skip": false,
                        "payload": {
                            "name": "test-firmware",
                            "version": "1.0",
                            "remoteUrl": "http://www.my.url"
                        }
                    },
                    {
                        "operation": "software_update",
                        "skip": false,
                        "payload": {
                            "updateList": [
                                {
                                    "type": "apt",
                                    "modules": [
                                        {
                                            "name": "test-software-1",
                                            "version": "latest",
                                            "action": "install"
                                        },
                                        {
                                            "name": "test-software-2",
                                            "version": "latest",
                                            "action": "install"
                                        }
                                    ]
                                }
                            ]
                        }
                    },
                    {
                        "operation": "config_update",
                        "skip": false,
                        "payload": {
                            "type": "path/config/test-software-1",
                            "remoteUrl":"http://www.my.url"
                        }
                    }
                ]
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `501` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us/child1", "501,c8y_DeviceProfile")])
            .await;

        // Expect `121` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us/child1", "121,false")]).await;

        // Simulate config_snapshot command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/device_profile/c8y-mapper-123456"),
            json!({
                "status": "failed",
                "reason": "Something went wrong",
                "name": "test-profile",
                "operations": [
                    {
                        "operation": "firmware_update",
                        "skip": false,
                        "payload": {
                            "name": "test-firmware",
                            "version": "1.0",
                            "remoteUrl": "http://www.my.url"
                        }
                    },
                    {
                        "operation": "software_update",
                        "skip": false,
                        "payload": {
                            "updateList": [
                                {
                                    "type": "apt",
                                    "modules": [
                                        {
                                            "name": "test-software-1",
                                            "version": "latest",
                                            "action": "install"
                                        },
                                        {
                                            "name": "test-software-2",
                                            "version": "latest",
                                            "action": "install"
                                        }
                                    ]
                                }
                            ]
                        }
                    },
                    {
                        "operation": "config_update",
                        "skip": false,
                        "payload": {
                            "type": "path/config/test-software-1",
                            "remoteUrl":"http://www.my.url"
                        }
                    }
                ]
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `502` smartrest message on `c8y/s/us`.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/child1",
                "502,c8y_DeviceProfile,Something went wrong",
            )],
        )
        .await;

        // An updated list of software is requested
        assert_received_contains_str(
            &mut mqtt,
            [(
                "te/device/child1///cmd/software_list/+",
                r#"{"status":"init"}"#,
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_device_profile_successful_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate config_update command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/device_profile/c8y-mapper-123456"),
            json!({
                "status": "successful",
                "name": "test-profile",
                "operations": [
                    {
                        "operation": "firmware_update",
                        "skip": false,
                        "payload": {
                            "name": "test-firmware",
                            "version": "1.0",
                            "remoteUrl": "http://www.my.url"
                        }
                    },
                    {
                        "operation": "software_update",
                        "skip": false,
                        "payload": {
                            "updateList": [
                                {
                                    "type": "apt",
                                    "modules": [
                                        {
                                            "name": "test-software-1",
                                            "version": "latest",
                                            "action": "install"
                                        },
                                        {
                                            "name": "test-software-2",
                                            "version": "latest",
                                            "action": "install"
                                        }
                                    ]
                                }
                            ]
                        }
                    },
                    {
                        "operation": "config_update",
                        "skip": false,
                        "payload": {
                            "type": "path/config/test-software-1",
                            "remoteUrl":"http://www.my.url"
                        }
                    }
                ]
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `121` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "121,true")]).await;

        // Expect `503` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "503,c8y_DeviceProfile")]).await;

        // An updated list of software is requested
        assert_received_contains_str(
            &mut mqtt,
            [(
                "te/device/main///cmd/software_list/+",
                r#"{"status":"init"}"#,
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_device_profile_successful_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // The child device must be registered first
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            r#"{ "@type":"child-device", "@id":"child1" }"#,
        ))
        .await
        .expect("fail to register the child-device");

        mqtt.skip(1).await; // Skip child device registration messages

        // Simulate config_update command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/device_profile/c8y-mapper-123456"),
            json!({
                "status": "successful",
                "name": "test-profile",
                "operations": [
                    {
                        "operation": "firmware_update",
                        "skip": false,
                        "payload": {
                            "name": "test-firmware",
                            "version": "1.0",
                            "remoteUrl": "http://www.my.url"
                        }
                    },
                    {
                        "operation": "software_update",
                        "skip": false,
                        "payload": {
                            "updateList": [
                                {
                                    "type": "apt",
                                    "modules": [
                                        {
                                            "name": "test-software-1",
                                            "version": "latest",
                                            "action": "install"
                                        },
                                        {
                                            "name": "test-software-2",
                                            "version": "latest",
                                            "action": "install"
                                        }
                                    ]
                                }
                            ]
                        }
                    },
                    {
                        "operation": "config_update",
                        "skip": false,
                        "payload": {
                            "type": "path/config/test-software-1",
                            "remoteUrl":"http://www.my.url"
                        }
                    }
                ]
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `121` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us/child1", "121,true")]).await;

        // Expect `503` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us/child1", "503,c8y_DeviceProfile")])
            .await;

        // An updated list of software is requested
        assert_received_contains_str(
            &mut mqtt,
            [(
                "te/device/child1///cmd/software_list/+",
                r#"{"status":"init"}"#,
            )],
        )
        .await;
    }
}
