## ADDED Requirements

### Requirement: Payload size limit is measured as the full MQTT packet size

The MQTT payload size limit SHALL be evaluated against the full MQTT PUBLISH packet wire size, not the message body alone. The wire size SHALL be computed by a single shared function and comprises the fixed-header control byte, the remaining-length variable-byte integer, the topic name and its two-byte length prefix, the two-byte packet identifier when QoS is greater than zero, and the payload bytes.

All payload size checks across the codebase SHALL use this shared function so that a value is interpreted identically everywhere.

#### Scenario: Wire size accounts for topic and framing
- **WHEN** the wire size of a PUBLISH is computed for a topic and payload
- **THEN** the result SHALL include the topic length, the framing overhead, and the payload, and SHALL equal the number of bytes the message occupies on the wire

#### Scenario: A message within the body limit but over the packet limit is rejected
- **WHEN** a message whose payload alone is within the limit but whose full packet size exceeds the limit is evaluated
- **THEN** it SHALL be treated as exceeding the limit

### Requirement: The bridge enforces the payload size limit on cloud-bound messages

The MQTT bridge SHALL enforce a configured `max_payload_size` on messages forwarded from the local broker to the cloud broker. A message whose wire size exceeds the limit SHALL NOT be forwarded to the cloud. The bridge SHALL acknowledge the over-limit message to the local broker so that it is not redelivered, and SHALL log the offending message's topic and size together with the configured limit.

The limit SHALL apply only to the local→cloud direction. Messages forwarded from the cloud to the local broker SHALL NOT be subject to this limit.

#### Scenario: Over-limit cloud-bound message is dropped, not forwarded
- **WHEN** a message received from the local broker has a wire size exceeding the bridge's `max_payload_size`
- **THEN** the bridge SHALL acknowledge it locally, SHALL NOT publish it to the cloud, and SHALL log its topic and size

#### Scenario: Over-limit message does not block subsequent traffic
- **WHEN** an over-limit message is followed by within-limit messages on bridged topics
- **THEN** the within-limit messages SHALL be forwarded to the cloud without disruption

#### Scenario: Cloud-to-local messages are not limited
- **WHEN** a message received from the cloud broker exceeds `max_payload_size`
- **THEN** the bridge SHALL forward it to the local broker

### Requirement: Built-in cloud connections enforce their configured limit in the bridge

For the built-in `c8y`, `az`, and `aws` cloud connections, the bridge SHALL enforce the cloud's configured `max_payload_size` value. This is the same value used by the mapper to limit the messages it generates, ensuring messages from any local source — not only the mapper — are checked before being sent to the cloud.

#### Scenario: Non-mapper publisher is checked by the bridge
- **WHEN** a local client publishes an over-limit message directly onto a topic bridged to a built-in cloud
- **THEN** the bridge SHALL drop it according to the cloud's configured `max_payload_size`, even though the message never passed through the mapper
