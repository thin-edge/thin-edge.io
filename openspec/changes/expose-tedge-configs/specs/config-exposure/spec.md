## ADDED Requirements

### Requirement: Opt-in config exposure
A configuration setting SHALL be published or served only if explicitly marked
as exposable where it is defined.
Settings holding sensitive material (private keys, PINs, credential-file paths)
SHALL NOT be marked exposable.

#### Scenario: Unmarked setting is never visible
- **WHEN** a client queries a setting that has no exposable marking
- **THEN** it SHALL NOT appear on any retained MQTT topic or HTTP response

#### Scenario: Marked setting is published and served
- **WHEN** a setting is marked exposable and has a value
- **THEN** it SHALL appear as a retained MQTT message under its owner's service topic
  and be available over the agent's HTTP API

### Requirement: Per-key retained MQTT publication under the owning service
Each exposable value SHALL be published as a retained message on
`te/device/<device>/service/<service>/config/<key>`,
where `<service>` is the owning component's service topic
and `<key>` is the config key with the cloud/profile prefix stripped.
An unset exposable value SHALL be published as an empty retained payload.
Each component SHALL publish only the settings it owns — the agent publishes
core/device settings, each mapper publishes only its own cloud settings.

#### Scenario: Core and cloud settings on the right service topics
- **WHEN** the agent starts with `device.id = my-device-01`
  and the c8y mapper starts with `c8y.url = example.cumulocity.com`
- **THEN** the agent SHALL publish retained on
  `.../tedge-agent/config/device.id` with payload `my-device-01`
- **AND** the c8y mapper SHALL publish retained on
  `.../tedge-mapper-c8y/config/url` with payload `example.cumulocity.com`

#### Scenario: A profiled mapper instance publishes under its own service topic
- **WHEN** a c8y mapper profile named `edge` is configured with a distinct bridge topic prefix `c8y-edge`
  and `c8y.profiles.edge.url` set to `edge.c8y.io`
- **THEN** it SHALL publish a retained message on
  `te/device/main/service/tedge-mapper-c8y-edge/config/url` with payload `edge.c8y.io`


### Requirement: Read-only HTTP view served by the agent
The agent SHALL subscribe to every service's `config/+` topics and serve
them as:
- `GET /te/v1/entities/<service-topic-id>/config` — JSON object of all
  exposed values for that service
- `GET /te/v1/entities/<service-topic-id>/config/<key>` — single value

A key that is not exposed and a key that does not exist SHALL both return
`404 Not Found`, indistinguishably.
Writes (PUT/PATCH/DELETE) SHALL be rejected.

#### Scenario: Whole-service and single-key queries
- **WHEN** a client sends `GET .../tedge-mapper-c8y/config`
- **THEN** the response SHALL be a JSON object of every exposed key-value pair
- **WHEN** a client sends `GET .../tedge-agent/config/device.id`
- **THEN** the response SHALL be the value `my-device-01`

### Requirement: Self-correcting publisher
Each component SHALL subscribe to its own `config/+` and reconcile every
message against its expected state, so that:
- an externally overwritten value is republished within one round trip
- a key removed or demoted between versions is cleared
- the component's own matching echo does not trigger further action

#### Scenario: Externally overwritten value is corrected
- **WHEN** a client publishes a different value on a component's own
  `config/<key>` topic
- **THEN** the owning component SHALL republish its own value on that topic

#### Scenario: A renamed or removed key's old retained value is cleared
- **WHEN** a key that was previously exposed is no longer in the current version's exposed-key set,
  because it was renamed or removed, and its old value is still retained in the broker
- **THEN** the component SHALL publish an empty retained payload on that key's old topic, clearing it

#### Scenario: A demoted key's retained value is cleared the same way
- **WHEN** a setting's `exposable` marking is removed while the setting itself still exists
- **THEN** it SHALL be treated the same as a removed key: its old retained value SHALL be cleared
