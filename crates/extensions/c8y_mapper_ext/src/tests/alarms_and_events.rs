use super::*;
use test_case::test_case;

#[test_case(
    "main",
    "temperature_alarm",
    r#"{ "severity": "major", "text": "Temperature high" }"#,
    "302,temperature_alarm,Temperature high"
    ; "main device"
)]
#[test_case(
    "external_sensor",
    "temperature_high",
    r#"{ "severity": "minor", "text": "Temperature high" }"#,
    "303,temperature_high,Temperature high"
    ; "child device"
)]
#[test_case(
    "main",
    "empty_temperature_alarm",
    "",
    "306,empty_temperature_alarm"
    ; "main device clearing an alarm"
)]
#[test_case(
    "external_sensor",
    "empty_temperature_alarm",
    "",
    "306,empty_temperature_alarm"
    ; "child device clearing an alarm"
)]
#[test_case(
    "main",
    "empty_temperature_alarm",
    "{}",
    "303,empty_temperature_alarm,empty_temperature_alarm"
    ; "main device with an empty json payload"
)]
#[tokio::test]
async fn c8y_mapper_alarm_mapping_to_smartrest(
    device: &str,
    alarm_type: &str,
    alarm: &str,
    expected: &str,
) {
    let (_ttd, mut mqtt) = spawn_mapper().await;
    let smartrest_topic = if device == "main" {
        "c8y/s/us".to_string()
    } else {
        register_external_sensor(&mut mqtt).await;
        format!("c8y/s/us/test-device:device:{device}")
    };

    publish(
        &mut mqtt,
        &format!("te/device/{device}///a/{alarm_type}"),
        alarm,
    )
    .await;

    assert_received_contains_str(&mut mqtt, [(smartrest_topic.as_str(), expected)]).await;
}

#[test_case(
    "main",
    "custom_temperature_alarm",
    json!({
        "severity": "major",
        "text": "Temperature high",
        "time":"2023-01-25T18:41:14.776170774Z",
        "customFragment": { "nested": { "value": "extra info" } }
    }),
    json!({
        "severity":"MAJOR",
        "time":"2023-01-25T18:41:14.776170774Z",
        "text":"Temperature high",
        "customFragment": { "nested": { "value":"extra info" } }
    })
    ; "main device with a custom fragment"
)]
#[test_case(
    "external_sensor",
    "custom_temperature_alarm",
    json!({
        "severity": "major",
        "text": "Temperature high",
        "time":"2023-01-25T18:41:14.776170774Z",
        "customFragment": { "nested": { "value": "extra info" } }
    }),
    json!({
        "severity":"MAJOR",
        "time":"2023-01-25T18:41:14.776170774Z",
        "text":"Temperature high",
        "customFragment": { "nested": { "value":"extra info" } }
    })
    ; "child device with a custom fragment"
)]
#[test_case(
    "main",
    "custom_msg_pressure_alarm",
    json!({
        "severity": "major",
        "text": "Pressure high",
        "time":"2023-01-25T18:41:14.776170774Z",
        "message":"custom message"
    }),
    json!({
        "severity":"MAJOR",
        "time":"2023-01-25T18:41:14.776170774Z",
        "text":"Pressure high",
        "message":"custom message"
    })
    ; "main device with a message as custom fragment"
)]
#[test_case(
    "external_sensor",
    "child_custom_msg_pressure_alarm",
    json!({
        "severity": "major",
        "text": "Pressure high",
        "time":"2023-01-25T18:41:14.776170774Z",
        "message":"custom message"
    }),
    json!({
        "severity":"MAJOR",
        "time":"2023-01-25T18:41:14.776170774Z",
        "text":"Pressure high",
        "message":"custom message"
    })
    ; "child device with a message as custom fragment"
)]
#[test_case(
    "main",
    "msg_to_text_pressure_alarm",
    json!({
        "severity": "major",
        "time":"2023-01-25T18:41:14.776170774Z",
        "message":"Pressure high"
    }),
    json!({
        "severity":"MAJOR",
        "time":"2023-01-25T18:41:14.776170774Z",
        "text":"msg_to_text_pressure_alarm",
        "message":"Pressure high"
    })
    ; "main device with a message but no text"
)]
#[test_case(
    "external_sensor",
    "child_msg_to_text_pressure_alarm",
    json!({
        "severity": "major",
        "time":"2023-01-25T18:41:14.776170774Z",
        "message":"Pressure high"
    }),
    json!({
        "severity":"MAJOR",
        "time":"2023-01-25T18:41:14.776170774Z",
        "text":"child_msg_to_text_pressure_alarm",
        "message":"Pressure high"
    })
    ; "child device with a message but no text"
)]
#[tokio::test]
async fn c8y_mapper_alarm_mapping_to_c8y_json(
    device: &str,
    alarm_type: &str,
    alarm: serde_json::Value,
    mut expected: serde_json::Value,
) {
    let (_ttd, mut mqtt) = spawn_mapper().await;
    expected["type"] = alarm_type.into();
    if device != "main" {
        register_external_sensor(&mut mqtt).await;
        expected["externalSource"] = json!({
            "externalId": format!("test-device:device:{device}"),
            "type": "c8y_Serial"
        });
    }

    let topic = format!("te/device/{device}///a/{alarm_type}");
    publish(&mut mqtt, &topic, alarm.to_string()).await;

    assert_received_includes_json(&mut mqtt, [("c8y/alarm/alarms/create", expected)]).await;
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

#[test_case(
    &[("te/device/external_sensor//", r#"{"@type": "child-device"}"#)],
    "te/device/external_sensor///e/custom_event",
    "test-device:device:external_sensor"
    ; "child device"
)]
#[test_case(
    &[
        ("te/device/external_sensor//", r#"{"@type": "child-device"}"#),
        ("te/device/external_sensor/service/service_child", r#"{"@type": "child-device"}"#),
    ],
    "te/device/external_sensor/service/service_child/e/custom_event",
    "test-device:device:external_sensor:service:service_child"
    ; "child device service"
)]
#[test_case(
    &[("te/device/main/service/service_main", r#"{"@type": "service"}"#)],
    "te/device/main/service/service_main/e/custom_event",
    "test-device:device:main:service:service_main"
    ; "main device service"
)]
#[tokio::test]
async fn c8y_mapper_entity_event(
    registrations: &[(&str, &str)],
    event_topic: &str,
    external_id: &str,
) {
    let (_ttd, mut mqtt) = spawn_mapper().await;
    for (topic, registration) in registrations {
        publish(&mut mqtt, topic, *registration).await;
    }

    let event = json!({
        "text": "Someone logged-in",
        "time":"2023-01-25T18:41:14.776170774Z",
    });
    publish(&mut mqtt, event_topic, event.to_string()).await;

    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/event/events/create",
            json!({
                "type":"custom_event",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Someone logged-in",
                "externalSource": {
                    "externalId": external_id,
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[test_case(
    &[
        ("te/device/external_sensor//", r#"{"@type": "child-device"}"#),
        ("te/device/external_sensor/service/service_child", r#"{"@type": "service"}"#),
    ],
    "te/device/external_sensor/service/service_child/a/custom_alarm",
    "c8y/s/us/test-device:device:external_sensor:service:service_child"
    ; "child device service"
)]
#[test_case(
    &[("te/device/main/service/service_main", r#"{"@type": "service"}"#)],
    "te/device/main/service/service_main/a/custom_alarm",
    "c8y/s/us/test-device:device:main:service:service_main"
    ; "main device service"
)]
#[tokio::test]
async fn c8y_mapper_service_alarm(
    registrations: &[(&str, &str)],
    alarm_topic: &str,
    smartrest_topic: &str,
) {
    let (_ttd, mut mqtt) = spawn_mapper().await;
    for (topic, registration) in registrations {
        publish(&mut mqtt, topic, *registration).await;
    }

    let alarm = json!({
        "severity":"critical",
        "text": "temperature alarm",
        "time":"2023-01-25T18:41:14.776170774Z",
    });
    publish(&mut mqtt, alarm_topic, alarm.to_string()).await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            smartrest_topic,
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

    register_nested_child(&mut mqtt).await;

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

    register_nested_child(&mut mqtt).await;

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

    register_nested_service(&mut mqtt).await;

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

    register_nested_service(&mut mqtt).await;

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

/// Waits for a retained message on the given topic, with a payload matching the given predicate
async fn assert_received_retained<M>(
    mqtt: &mut TestMqttBox<M>,
    topic: &str,
    payload_matches: impl Fn(&[u8]) -> bool,
) where
    M: MessageReceiver<MqttMessage> + Send,
{
    let is_expected =
        |m: &MqttMessage| m.topic.name == topic && m.retain && payload_matches(m.payload_bytes());
    // Messages might have already been buffered by a previous assertion
    while !mqtt.any(is_expected) {
        if mqtt.recv().await.is_none() {
            panic!("Didn't receive the expected retained message on {topic}");
        }
    }
}

/// Registers the `external_sensor` child device of the main device
async fn register_external_sensor<M: Sender<MqttMessage>>(mqtt: &mut TestMqttBox<M>) {
    publish(
        mqtt,
        "te/device/external_sensor//",
        r#"{"@type": "child-device"}"#,
    )
    .await;
}

/// Registers the `nested_child` device, a child of `immediate_child`, itself a child of the main device
async fn register_nested_child<M: Sender<MqttMessage>>(mqtt: &mut TestMqttBox<M>) {
    let immediate_child = json!({
        "@type":"child-device",
        "@parent":"device/main//",
        "@id":"immediate_child"
    });
    publish(
        mqtt,
        "te/device/immediate_child//",
        immediate_child.to_string(),
    )
    .await;
    let nested_child = json!({
        "@type":"child-device",
        "@parent":"device/immediate_child//",
        "@id":"nested_child"
    });
    publish(mqtt, "te/device/nested_child//", nested_child.to_string()).await;
}

/// Registers the `nested_service` service of the `nested_child` device
async fn register_nested_service<M: Sender<MqttMessage>>(mqtt: &mut TestMqttBox<M>) {
    register_nested_child(mqtt).await;
    let nested_service = json!({
        "@type":"service",
        "@parent":"device/nested_child//",
        "@id":"nested_service"
    });
    publish(
        mqtt,
        "te/device/nested_child/service/nested_service",
        nested_service.to_string(),
    )
    .await;
}
