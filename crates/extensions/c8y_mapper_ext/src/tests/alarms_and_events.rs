use super::*;

#[tokio::test]
async fn c8y_mapper_alarm_mapping_to_smartrest() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    publish(
        &mut mqtt,
        "te/device/main///a/temperature_alarm",
        r#"{ "severity": "major", "text": "Temperature high" }"#,
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "302,temperature_alarm,Temperature high")],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_alarm_mapping_to_smartrest() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    // Register the device upfront
    publish(
        &mut mqtt,
        "te/device/external_sensor//",
        r#"{"@type": "child-device"}"#,
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/external_sensor///a/temperature_high",
        json!({ "severity": "minor", "text": "Temperature high" }).to_string(),
    )
    .await;

    // Expect child device creation and converted temperature alarm messages
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:external_sensor",
            "303,temperature_high,Temperature high",
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_with_custom_fragment_mapping_to_c8y_json() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    publish(
        &mut mqtt,
        "te/device/main///a/custom_temperature_alarm",
        json!({
            "severity": "major",
            "text": "Temperature high",
            "time":"2023-01-25T18:41:14.776170774Z",
            "customFragment": {
                "nested": {
                    "value": "extra info"
                }
            }
        })
        .to_string(),
    )
    .await;

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/alarm/alarms/create",
            json!({
                "type":"custom_temperature_alarm",
                "severity":"MAJOR",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Temperature high",
                "customFragment": {
                    "nested": {
                        "value":"extra info"
                    }
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_alarm_with_custom_fragment_mapping_to_c8y_json() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    // Register the device upfront
    publish(
        &mut mqtt,
        "te/device/external_sensor//",
        r#"{"@type": "child-device"}"#,
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/external_sensor///a/custom_temperature_alarm",
        json!({
            "severity": "major",
            "text": "Temperature high",
            "time":"2023-01-25T18:41:14.776170774Z",
            "customFragment": {
                "nested": {
                    "value": "extra info"
                }
            }
        })
        .to_string(),
    )
    .await;

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/alarm/alarms/create",
            json!({
                "type":"custom_temperature_alarm",
                "severity":"MAJOR",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Temperature high",
                "customFragment": {
                    "nested": {
                        "value":"extra info"
                    }
                },
                "externalSource": {
                    "externalId":"test-device:device:external_sensor",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_with_message_as_custom_fragment_mapping_to_c8y_json() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    publish(
        &mut mqtt,
        "te/device/main///a/custom_msg_pressure_alarm",
        json!({
            "severity": "major",
            "text": "Pressure high",
            "time":"2023-01-25T18:41:14.776170774Z",
            "message":"custom message"
        })
        .to_string(),
    )
    .await;

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/alarm/alarms/create",
            json!({
                "type":"custom_msg_pressure_alarm",
                "severity":"MAJOR",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Pressure high",
                "message":"custom message"
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_alarm_with_message_custom_fragment_mapping_to_c8y_json() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    // Register the device upfront
    publish(
        &mut mqtt,
        "te/device/external_sensor//",
        r#"{"@type": "child-device"}"#,
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/external_sensor///a/child_custom_msg_pressure_alarm",
        json!({
            "severity": "major",
            "text": "Pressure high",
            "time":"2023-01-25T18:41:14.776170774Z",
            "message":"custom message"
        })
        .to_string(),
    )
    .await;

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/alarm/alarms/create",
            json!({
                "type":"child_custom_msg_pressure_alarm",
                "severity":"MAJOR",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Pressure high",
                "message":"custom message",
                "externalSource":{
                    "externalId":"test-device:device:external_sensor",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_alarm_with_custom_message() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    // Register the device upfront
    publish(
        &mut mqtt,
        "te/device/external_sensor//",
        r#"{"@type": "child-device"}"#,
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/external_sensor///a/child_msg_to_text_pressure_alarm",
        json!({
            "severity": "major",
            "time":"2023-01-25T18:41:14.776170774Z",
            "message":"Pressure high"
        })
        .to_string(),
    )
    .await;

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/alarm/alarms/create",
            json!({
                "type":"child_msg_to_text_pressure_alarm",
                "severity":"MAJOR",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"child_msg_to_text_pressure_alarm",
                "message":"Pressure high",
                "externalSource":{
                    "externalId":"test-device:device:external_sensor",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_with_custom_message() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    publish(
        &mut mqtt,
        "te/device/main///a/msg_to_text_pressure_alarm",
        json!({
            "severity": "major",
            "time":"2023-01-25T18:41:14.776170774Z",
            "message":"Pressure high"
        })
        .to_string(),
    )
    .await;

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/alarm/alarms/create",
            json!({
                "type":"msg_to_text_pressure_alarm",
                "severity":"MAJOR",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"msg_to_text_pressure_alarm",
                "message":"Pressure high",
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_alarm_empty_payload() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    // Register the device upfront
    publish(
        &mut mqtt,
        "te/device/external_sensor//",
        r#"{"@type": "child-device"}"#,
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/external_sensor///a/empty_temperature_alarm",
        "".to_string(),
    )
    .await;

    // Expect converted alarm SmartREST message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:external_sensor",
            "306,empty_temperature_alarm",
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_empty_payload() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    publish(
        &mut mqtt,
        "te/device/main///a/empty_temperature_alarm",
        "".to_string(),
    )
    .await;

    // Expect converted alarm SmartREST message
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "306,empty_temperature_alarm")]).await;
}

#[tokio::test]
async fn c8y_mapper_alarm_is_persisted_to_internal_topic() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    publish(
        &mut mqtt,
        "te/device/main///a/temperature_alarm",
        r#"{ "severity": "major", "text": "Temperature high" }"#,
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "302,temperature_alarm,Temperature high")],
    )
    .await;

    // The alarms flow subscribes to its own internal topic to reconcile alarms on restart,
    // so the retained copy of the alarm must not be dropped by the flow loop detection
    assert_received_retained(
        &mut mqtt,
        "c8y-internal/alarms/te/device/main///a/temperature_alarm",
        |payload| {
            let alarm: serde_json::Value = serde_json::from_slice(payload).unwrap();
            alarm["severity"] == "major" && alarm["text"] == "Temperature high"
        },
    )
    .await;

    // Clearing the alarm must also clear its internal copy,
    // otherwise the mapper would clear the alarm again on every restart
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/main///a/temperature_alarm"),
            "",
        )
        .with_retain(),
    )
    .await
    .unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "306,temperature_alarm")]).await;
    assert_received_retained(
        &mut mqtt,
        "c8y-internal/alarms/te/device/main///a/temperature_alarm",
        |payload| payload.is_empty(),
    )
    .await;
}

/// Wait for a retained message on the given topic, with a payload matching the given predicate
async fn assert_received_retained<M>(
    mqtt: &mut TestMqttBox<M>,
    topic: &str,
    payload_matches: impl Fn(&[u8]) -> bool,
) where
    M: MessageReceiver<MqttMessage> + Send,
{
    // Messages might have already been buffered by a previous assertion
    while !mqtt.any(|m| m.topic.name == topic && m.retain && payload_matches(m.payload_bytes())) {
        if mqtt.recv().await.is_none() {
            panic!("Didn't receive the expected retained message on {topic}");
        }
    }
}

#[tokio::test]
async fn c8y_mapper_alarm_empty_json_payload() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    publish(
        &mut mqtt,
        "te/device/main///a/empty_temperature_alarm",
        "{}",
    )
    .await;

    // Expect converted alarm SmartREST message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "303,empty_temperature_alarm,empty_temperature_alarm",
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_event() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    // Register the device upfront
    publish(
        &mut mqtt,
        "te/device/external_sensor//",
        r#"{"@type": "child-device"}"#,
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/external_sensor///e/custom_event",
        json!({
            "text": "Someone logged-in",
            "time":"2023-01-25T18:41:14.776170774Z",
        })
        .to_string(),
    )
    .await;

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/event/events/create",
            json!({
                "type":"custom_event",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Someone logged-in",
                "externalSource": {
                    "externalId":"test-device:device:external_sensor",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_service_event() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    // Register the device upfront
    publish(
        &mut mqtt,
        "te/device/external_sensor//",
        r#"{"@type": "child-device"}"#,
    )
    .await;
    // Register the device upfront
    publish(
        &mut mqtt,
        "te/device/external_sensor/service/service_child",
        r#"{"@type": "child-device"}"#,
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/external_sensor/service/service_child/e/custom_event",
        json!({
            "text": "Someone logged-in",
            "time":"2023-01-25T18:41:14.776170774Z",
        })
        .to_string(),
    )
    .await;

    // Expect converted event message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/event/events/create",
            json!({
                "type":"custom_event",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Someone logged-in",
                "externalSource": {
                    "externalId":"test-device:device:external_sensor:service:service_child",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_main_service_event() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    // Register the service upfront
    publish(
        &mut mqtt,
        "te/device/main/service/service_main",
        r#"{"@type": "service"}"#,
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/main/service/service_main/e/custom_event",
        json!({
            "text": "Someone logged-in",
            "time":"2023-01-25T18:41:14.776170774Z",
        })
        .to_string(),
    )
    .await;

    // Expect converted event for the main device service
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/event/events/create",
            json!({
                "type":"custom_event",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Someone logged-in",
                "externalSource": {
                    "externalId":"test-device:device:main:service:service_main",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_service_alarm() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    // Register the device and service upfront
    publish(
        &mut mqtt,
        "te/device/external_sensor//",
        r#"{"@type": "child-device"}"#,
    )
    .await;
    publish(
        &mut mqtt,
        "te/device/external_sensor/service/service_child",
        r#"{"@type": "service"}"#,
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/external_sensor/service/service_child/a/custom_alarm",
        json!({
            "severity":"critical",
            "text": "temperature alarm",
            "time":"2023-01-25T18:41:14.776170774Z",
        })
        .to_string(),
    )
    .await;

    // Expect converted alarm for the main device service
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:external_sensor:service:service_child",
            "301,custom_alarm,temperature alarm,2023-01-25T18:41:14.776170774Z",
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_main_service_alarm() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    // Register the service upfront
    publish(
        &mut mqtt,
        "te/device/main/service/service_main",
        r#"{"@type": "service"}"#,
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/main/service/service_main/a/custom_alarm",
        json!({
            "severity":"critical",
            "text": "temperature alarm",
            "time":"2023-01-25T18:41:14.776170774Z",
        })
        .to_string(),
    )
    .await;

    // Expect converted alarm for the main device service
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:main:service:service_main",
            "301,custom_alarm,temperature alarm,2023-01-25T18:41:14.776170774Z",
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_complex_text_fragment_in_payload_failed() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    publish(
        &mut mqtt,
        "te/device/main///a/complex_text_alarm",
        json!({
            "severity": "major",
            "text":{
                "nested":{
                    "value":"extra info"
                }
            },
            "time":"2023-01-25T18:41:14.776170774Z",
            "message":"custom message"
        })
        .to_string(),
    )
    .await;

    // Expect converted alarm SmartREST message
    assert_received_contains_str(&mut mqtt, [("te/errors", "Parsing of alarm message for the type: complex_text_alarm failed due to error: invalid")]).await;
}

#[tokio::test]
async fn c8y_mapper_nested_child_alarm_mapping_to_smartrest() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/immediate_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/main//",
            "@id":"immediate_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/immediate_child//",
            "@id":"nested_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    publish(&mut mqtt, "te/device/nested_child///a/", json!({ "severity": "minor", "text": "Temperature high","time":"2023-10-13T15:00:07.172674353Z" }).to_string()).await;

    // Expect nested child device creating an minor alarm
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "101,immediate_child,immediate_child,thin-edge.io-child,false",
            ),
            (
                "c8y/s/us/immediate_child",
                "101,nested_child,nested_child,thin-edge.io-child,false",
            ),
            (
                "c8y/s/us/nested_child",
                "303,ThinEdgeAlarm,Temperature high,2023-10-13T15:00:07.172674353Z",
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_nested_child_event_mapping_to_smartrest() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/immediate_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/main//",
            "@id":"immediate_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/immediate_child//",
            "@id":"nested_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    publish(
        &mut mqtt,
        "te/device/nested_child///e/",
        json!({ "text": "Temperature high","time":"2023-10-13T15:00:07.172674353Z" }).to_string(),
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "101,immediate_child,immediate_child,thin-edge.io-child,false",
            ),
            (
                "c8y/s/us/immediate_child",
                "101,nested_child,nested_child,thin-edge.io-child,false",
            ),
        ],
    )
    .await;
    // Expect nested child device creating an event
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/event/events/create",
            json!({
                "type":"ThinEdgeEvent",
                "time":"2023-10-13T15:00:07.172674353Z",
                "text":"Temperature high",
                "externalSource":{"externalId":"nested_child","type":"c8y_Serial"}
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_nested_child_service_alarm_mapping_to_smartrest() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/immediate_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/main//",
            "@id":"immediate_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/immediate_child//",
            "@id":"nested_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child/service/nested_service"),
        json!({
            "@type":"service",
            "@parent":"device/nested_child//",
            "@id":"nested_service"
        })
        .to_string(),
    );

    mqtt.send(reg_message).await.unwrap();

    publish(&mut mqtt, "te/device/nested_child/service/nested_service/a/", json!({ "severity": "minor", "text": "Temperature high","time":"2023-10-13T15:00:07.172674353Z" }).to_string()).await;

    // Expect child device service creating minor temperature alarm messages
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/nested_service",
            "303,ThinEdgeAlarm,Temperature high,2023-10-13T15:00:07.172674353Z",
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_nested_child_service_event_mapping_to_smartrest() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/immediate_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/main//",
            "@id":"immediate_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/immediate_child//",
            "@id":"nested_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child/service/nested_service"),
        json!({
            "@type":"service",
            "@parent":"device/nested_child//",
            "@id":"nested_service"
        })
        .to_string(),
    );

    mqtt.send(reg_message).await.unwrap();

    publish(
        &mut mqtt,
        "te/device/nested_child/service/nested_service/e/",
        json!({ "text": "Temperature high","time":"2023-10-13T15:00:07.172674353Z" }).to_string(),
    )
    .await;

    // Expect nested child device service creating the event messages
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/event/events/create",
            json!({
                "type":"ThinEdgeEvent",
                "time":"2023-10-13T15:00:07.172674353Z",
                "text":"Temperature high",
                "externalSource":{"externalId":"nested_service","type":"c8y_Serial"}
            }),
        )],
    )
    .await;
}
