#[cfg(test)]
mod tests {
    use crate::tests::skip_init_messages;
    use crate::tests::spawn_c8y_mapper_actor;
    use crate::tests::TestHandle;

    use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
    use serde_json::json;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::Sender;
    use tedge_mqtt_ext::test_helpers::assert_received_includes_json;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

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
                                "url": "http://www.my.url"
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
                                "url": "http://www.my.url"
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
                        }
                    ]
                }),
            )],
        )
        .await;
    }
}
