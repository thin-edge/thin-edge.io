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

### Requirement: Aggregate retained MQTT publication under the owning service
Each service's exposable values SHALL be published as one retained JSON
message on `te/device/<device>/service/<service>/config`,
where `<service>` is the owning component's service topic
and each JSON field name is a config key with the cloud/profile prefix stripped.
Each value SHALL keep the type the setting has in `tedge.toml`,
so a port is a number, a flag is a boolean and a template set is an array.
An unset exposable key SHALL be omitted from the object.
Each component SHALL publish only the settings it owns — the agent publishes
core/device settings, each mapper publishes only its own cloud settings.

#### Scenario: Core and cloud settings on the right service topics
- **WHEN** the agent starts with `device.id = my-device-01`
  and the c8y mapper starts with `c8y.url = example.cumulocity.com`
- **THEN** the agent SHALL publish retained on `.../tedge-agent/config`
  a JSON object containing `"device.id":"my-device-01"`
- **AND** the c8y mapper SHALL publish retained on
  `.../tedge-mapper-c8y/config` a JSON object containing
  `"url":"example.cumulocity.com"`

#### Scenario: A non-string setting is published with its own type
- **WHEN** the agent starts with `mqtt.client.port = 1883`
  and the c8y mapper starts with `c8y.enable.log_upload = true`
  and `c8y.smartrest.templates = ["1234", "5678"]`
- **THEN** the published objects SHALL contain `"mqtt.client.port":1883`,
  `"enable.log_upload":true` and `"smartrest.templates":["1234","5678"]`
  rather than the string renderings of those values

#### Scenario: A profiled mapper instance publishes under its own service topic
- **WHEN** a c8y mapper profile named `edge` is configured with a distinct bridge topic prefix `c8y-edge`
  and `c8y.profiles.edge.url` set to `edge.c8y.io`
- **THEN** it SHALL publish a retained JSON message on
  `te/device/main/service/tedge-mapper-c8y-edge/config` containing
  `"url":"edge.c8y.io"`


### Requirement: Read-only HTTP view served by the agent
The agent SHALL subscribe to every service's `config` topic and serve
them as:
- `GET /te/v1/entities/<service-topic-id>/config` — JSON object of all
  exposed values for that service
- `GET /te/v1/entities/<service-topic-id>/config/<key>` — single value,
  looked up in that service's parsed object

Both routes SHALL respond with JSON, so each value keeps the type it has
in `tedge.toml`.
A key that is not exposed and a key that does not exist SHALL both return
`404 Not Found`, indistinguishably.
Writes (PUT/PATCH/DELETE) SHALL be rejected.

#### Scenario: Whole-service and single-key queries
- **WHEN** a client sends `GET .../tedge-mapper-c8y/config`
- **THEN** the response SHALL be a JSON object of every exposed key-value pair
- **WHEN** a client sends `GET .../tedge-agent/config/device.id`
- **THEN** the response SHALL be the JSON value `"my-device-01"`
- **WHEN** a client sends `GET .../tedge-agent/config/mqtt.client.port`
- **THEN** the response SHALL be the JSON value `1883`

### Requirement: Self-correcting publisher
Each component SHALL subscribe to its own `config` topic and republish its
expected JSON object whenever the retained payload doesn't match it, so
that:
- an externally overwritten or corrupted document is corrected within one
  round trip
- a key demoted, renamed, or removed between versions is absent from the
  republished document — no separate clearing step is needed
- the component's own matching echo does not trigger further action

#### Scenario: Externally overwritten document is corrected
- **WHEN** a client publishes a different payload on a component's own
  `config` topic
- **THEN** the owning component SHALL republish its own expected JSON object
  on that topic

#### Scenario: A renamed, removed, or demoted key is absent from the republished document
- **WHEN** a key that was previously exposed is no longer in the current
  version's exposed-key set — because it was renamed, removed, or its
  `exposable` marking was dropped — and the old retained document still
  contains it
- **THEN** the next republish SHALL omit that key, since the whole document
  is replaced rather than patched
