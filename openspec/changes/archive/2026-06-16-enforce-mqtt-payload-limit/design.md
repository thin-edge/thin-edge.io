## Context

The MQTT bridge (`crates/extensions/tedge_mqtt_bridge`) forwards messages between the local broker and a cloud broker. It does not enforce any payload-size limit. When a message exceeds the cloud broker's limit, the broker silently stops responding; the connection dies on keepalive timeout and, on reconnect with a fresh session, the bridge republishes the same message — an unbounded loop that blocks all cloud-bound traffic behind the offending message.

`max_payload_size` already exists for the built-in clouds (`c8y`/`az`/`aws` under `<cloud>.mapper.mqtt.max_payload_size`) but is only consulted by the mappers to limit the messages they generate. Three independent checks exist, each comparing only the message body length:

- `tedge_flows` `limit-payload-size` transformer (`crates/extensions/tedge_flows/src/transformers/limit_payload_size.rs`) — used by az/aws via a builtin flow.
- `c8y_mapper_ext::can_send_over_mqtt` (`crates/extensions/c8y_mapper_ext/src/mea/events.rs`).
- The bridge has none.

Body-only checks undercount: the cloud broker limits the full MQTT packet (topic + framing + payload). The mappers also cannot protect messages published directly by other local clients, plugins, or passthrough topics — only the bridge sees all cloud-bound traffic.

## Goals / Non-Goals

**Goals:**
- One shared function computes the MQTT PUBLISH wire size; every payload-size check uses it.
- The bridge enforces `max_payload_size` on cloud-bound messages and drops over-limit messages instead of letting them wedge the connection.
- Custom (user-defined) mappers can configure `max_payload_size`, which the bridge then enforces.

**Non-Goals:**
- Detecting or quarantining other classes of poison message (malformed packets, broker-specific rejections). Only the size limit is in scope.
- Splitting, compressing, or otherwise salvaging an over-limit message. An over-limit message is undeliverable and is dropped.
- Reducing detection latency of a silently-dropped connection (a keepalive concern).

## Decisions

### Wire-size function lives in `mqtt_channel`

`mqtt_channel` is the only crate that wraps `rumqttc`, it defines `MqttMessage`, and every consumer reaches it (`tedge_flows → tedge_mqtt_ext → mqtt_channel`; bridge → `mqtt_channel`; `c8y_mapper_ext → mqtt_channel`). The function computes `1 + len_len(remaining) + 2 + topic.len() + (qos>0 ? 2 : 0) + payload.len()`, matching `rumqttc::Publish::size()` — the exact byte count placed on the wire.

Alternative considered: a helper in `tedge_api`. Rejected — `tedge_api` does not wrap the wire format, so it would have to duplicate the framing arithmetic that `mqtt_channel`/`rumqttc` already own.

### The bridge enforces at forward time, on the local→cloud half only

The limit is applied in the `half_bridge` that forwards local messages to the cloud, at the point a received publish is converted and about to be forwarded. Over-limit messages are acknowledged to the local broker (reusing the existing local-ack path already used for non-forwarded messages) and not forwarded.

Catching the message before it is handed to the cloud client means it never enters the cloud event loop's pending/inflight queue, so there is nothing to clear there and the republish loop cannot begin. The cloud→local half is left unguarded: the local broker accepts large messages, and cloud-originated messages have already satisfied the cloud's own limit.

Alternative considered: reactively inspecting the inflight set after a disconnect to blame the offending message. Rejected for this change — it is heuristic, risks dropping a legitimate message, and only acts after the connection has already failed.

### One limit value, two enforcement points

The bridge reuses the same `max_payload_size` value the mapper already uses, rather than introducing a separate setting. The mapper continues to limit the messages it generates; the bridge backstops everything else. Because mapper output already satisfies the limit, the bridge only ever drops messages that bypassed the mapper.

### `max_payload_size` is threaded through `MqttBridgeActorBuilder::new`

The value is passed as a parameter to the builder rather than embedded in the bridge rules, keeping it explicit at each call site (c8y, az, aws, custom). Custom mappers gain a `max_payload_size` field on their `[bridge]` config, surfaced through `EffectiveMapperConfig`, defaulting to the MQTT maximum (`268435455`).

## Risks / Trade-offs

- Switching from body size to packet size tightens the effective limit by the topic + framing overhead → for built-in clouds the conservative defaults sit well below broker hard limits, so the few extra bytes are immaterial; covered by a test asserting packet-size semantics.
- Dropping an over-limit message loses that message → an over-limit message is undeliverable regardless; dropping it is strictly better than blocking all cloud-bound traffic. The drop is logged with topic and size for operator visibility.
- Custom mappers default to the MQTT maximum → the limit is effectively off until the operator sets a value appropriate to their broker, preserving existing behaviour for those mappers.
