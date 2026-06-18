## 1. Shared wire-size function

- [x] 1.1 Add a function to `mqtt_channel` computing the MQTT PUBLISH wire size for an `MqttMessage` (`1 + len_len(remaining) + 2 + topic.len() + (qos>0 ? 2 : 0) + payload.len()`), matching `rumqttc::Publish::size()`
- [x] 1.2 Unit-test the function against known byte counts, including a case where the body is within a limit but the full packet exceeds it, and a QoS-0 vs QoS-1 case

## 2. Adopt the shared function in existing checks

- [x] 2.1 Replace the body-only comparison in the `tedge_flows` `limit-payload-size` transformer with the shared wire-size function
- [x] 2.2 Replace the body-only comparison in `c8y_mapper_ext::can_send_over_mqtt` with the shared wire-size function
- [x] 2.3 Reconcile the comparison so both treat the boundary identically (over-limit = wire size strictly greater than the limit)
- [x] 2.4 Update or add unit tests covering the packet-size semantics for both call sites

## 3. Bridge enforcement

- [x] 3.1 Add a `max_payload_size` parameter to `MqttBridgeActorBuilder::new`
- [x] 3.2 Thread the limit into the local→cloud `half_bridge` only; leave the cloud→local direction unguarded
- [x] 3.3 In the local→cloud forward path, when a converted message's wire size exceeds the limit: acknowledge it to the local broker via the existing local-ack path, do not forward it, and log its topic and size with the configured limit
- [x] 3.4 Add bridge tests: over-limit cloud-bound message is acked and not forwarded; a following within-limit message is still forwarded; an over-limit cloud→local message is forwarded unchanged

## 4. Wire the limit through call sites

- [x] 4.1 Pass each built-in cloud's configured `max_payload_size` (c8y/az/aws) into the bridge builder
- [x] 4.2 Update bridge integration tests for the new `MqttBridgeActorBuilder::new` signature

## 5. Custom mapper configuration

- [x] 5.1 Add a `max_payload_size` field to the custom mapper `[bridge]` config (`crates/core/tedge_mapper/src/custom/config.rs`), defaulting to the MQTT maximum (`268435455`)
- [x] 5.2 Surface the value through `EffectiveMapperConfig` and pass it into the bridge builder from the custom mapper
- [x] 5.3 Test that a configured custom-mapper limit is enforced and that the default leaves it effectively disabled

## 6. Documentation

- [x] 6.1 Document the `max_payload_size` custom-mapper setting, noting that the limit measures the full MQTT packet rather than the body alone
