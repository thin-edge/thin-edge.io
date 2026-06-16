## Why

An oversized message forwarded to the cloud MQTT broker can wedge the bridge indefinitely: the broker silently stops responding, the connection dies on keepalive timeout, and on reconnect the same message is republished — head-of-line-blocking *all* cloud-bound traffic. The bridge does not enforce `max_payload_size` today, and the size checks that do exist measure only the message body, undercounting the true packet size the broker actually limits.

## What Changes

- Add a single shared function in `mqtt_channel` that computes the true MQTT PUBLISH wire size (control byte + remaining-length varint + topic + packet id + payload).
- Replace the three independent, body-only size checks (`tedge_flows` `limit-payload-size` transformer, `c8y_mapper_ext` `can_send_over_mqtt`, and the bridge's absence of one) with calls to that shared function.
- Enforce `max_payload_size` in the MQTT bridge on the local→cloud direction: a message whose wire size exceeds the limit is logged, acknowledged locally so it is not redelivered, and dropped rather than forwarded.
- Add `max_payload_size` to the custom (user-defined) mapper configuration, defaulting to the MQTT maximum so the limit is effectively off for brokers whose limit is unknown.
- **BREAKING**: the limit now measures the full MQTT packet (topic + framing + payload) rather than the payload body alone. A message whose body fits the limit but whose packet does not is now rejected.

## Capabilities

### New Capabilities
- `mqtt-payload-limit`: defines how the MQTT payload-size limit is calculated (full packet wire size) and enforced — both at message generation in the mappers and as a backstop in the bridge for all cloud-bound traffic.

### Modified Capabilities
- `custom-mapper-config`: the custom mapper configuration gains a `max_payload_size` bridge setting.

## Impact

- `crates/common/mqtt_channel` — new wire-size function on `MqttMessage`.
- `crates/extensions/tedge_mqtt_bridge` — `MqttBridgeActorBuilder::new` gains a `max_payload_size` parameter; the local→cloud half enforces it.
- `crates/extensions/tedge_flows` (`limit-payload-size` transformer) and `crates/extensions/c8y_mapper_ext` (`can_send_over_mqtt`) — switch to the shared wire-size function.
- `crates/core/tedge_mapper` — bridge call sites (c8y, az, aws, custom) pass the limit; custom mapper config/schema gains the new field.
- Bridge construction signature change affects bridge integration tests.
