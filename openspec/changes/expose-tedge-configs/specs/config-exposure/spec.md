## ADDED Requirements

### Requirement: Configuration exposure is opt-in per setting
A configuration setting SHALL be published or served to external clients only if it has been explicitly
marked as exposable where it is defined.
A setting with no such marking SHALL NOT be exposed, regardless of whether it holds a secret.

#### Scenario: Unmarked setting is never exposed
- **WHEN** a configuration setting has no exposable marking
- **THEN** it SHALL NOT appear on any retained MQTT topic or HTTP response, even if its value is set

#### Scenario: Marked setting is exposed when set
- **WHEN** a configuration setting is marked exposable and has a value
- **THEN** its value SHALL be published on its owner's retained MQTT topic and served over HTTP

#### Scenario: Secrets are never marked exposable
- **WHEN** a configuration setting holds sensitive material (a private key, a PIN, a credential file path)
- **THEN** it SHALL NOT be marked exposable, and SHALL therefore never be published or served

### Requirement: Exposed values are published as retained per-key MQTT messages
Each exposable configuration value SHALL be published as a single retained MQTT message on
`te/device/<device>/service/<service>/config/<key>`,
where `<service>` is the publishing component's own service topic
and `<key>` is the configuration key as it is known to that component (i.e. with its own cloud/profile
qualifier already stripped).
The payload SHALL be the value as a plain string.

#### Scenario: Core setting published under the agent's service topic
- **WHEN** the agent starts with `device.id` set to `my-device-01`
- **THEN** it SHALL publish a retained message on `te/device/main/service/tedge-agent/config/device.id`
  with payload `my-device-01`

#### Scenario: Cloud setting published under the mapper's service topic with its prefix stripped
- **WHEN** the c8y mapper starts with `c8y.url` set to `example.cumulocity.com`
- **THEN** it SHALL publish a retained message on `te/device/main/service/tedge-mapper-c8y/config/url`
  with payload `example.cumulocity.com`

#### Scenario: A profiled mapper instance publishes under its own service topic
- **WHEN** a c8y mapper profile named `edge` is configured with a distinct bridge topic prefix `c8y-edge`
  and `c8y.profiles.edge.url` set to `edge.c8y.io`
- **THEN** it SHALL publish a retained message on
  `te/device/main/service/tedge-mapper-c8y-edge/config/url` with payload `edge.c8y.io`

#### Scenario: A consumer subscribes to everything one service exposes
- **WHEN** a client subscribes to `te/device/main/service/tedge-mapper-c8y/config/+`
- **THEN** it SHALL receive every exposable value published by that mapper instance,
  and none owned by another component

### Requirement: Each component publishes only the configuration it owns
A component SHALL publish only the exposable settings it is the source of truth for:
the agent publishes core/device settings;
each mapper publishes only its own cloud's settings for its own profile.
No component SHALL read another component's configuration file or republish values on another
component's behalf.

#### Scenario: The agent does not publish cloud settings
- **WHEN** the agent starts
- **THEN** it SHALL NOT publish any `c8y.*`, `az.*`, or `aws.*` configuration value under its own service
  topic

#### Scenario: A mapper does not publish another cloud's settings
- **WHEN** the c8y mapper starts
- **THEN** it SHALL NOT publish any `az.*` or `aws.*` configuration value

### Requirement: An exposed value that becomes unset is actively cleared
When a component starts, it SHALL publish, for every exposable key in its scope, either the current
value or an empty retained payload if the key is unset —
so that a value removed from configuration does not linger from a previous run.

#### Scenario: A previously-set value is unset before restart
- **WHEN** an exposable setting had a value on a prior run, is unset before the next start,
  and the owning component restarts
- **THEN** the component SHALL publish an empty retained payload on that value's topic,
  clearing the stale retained message

### Requirement: A component reconciles the retained state of its own config topics
A component SHALL subscribe to its own service's `config/+` topics and, for every retained or incoming
message it receives there, restore the broker's retained state to the expected state it holds for that
key — `Some(value)` for a key currently exposed and set, and absent for every other key (an exposed key
that is unset, or any key not in its exposed set at all):
- if the expected state is a value and the payload does not equal it — **including an empty payload** —
  it SHALL republish the expected value;
- if the expected state is absent and the payload is non-empty, it SHALL clear the topic with an empty
  retained payload;
- if the payload already equals the expected state (a matching value, or empty where the state is
  absent), it SHALL take no action.

Reconciling against expected state, not special-casing empty payloads, corrects external tampering
(including an empty payload that would wipe an owned value) and clears values left behind by a renamed,
removed, or demoted key.

#### Scenario: An externally published value is corrected
- **WHEN** a client other than the owning component publishes a different value on that component's own
  `config/<key>` topic
- **THEN** the owning component SHALL detect the divergence and republish its own value on that topic

#### Scenario: An externally published empty payload does not wipe an owned value
- **WHEN** a client publishes an empty payload on a `config/<key>` topic for a key the component
  currently exposes with a value
- **THEN** the component SHALL treat it as divergence from its expected value and republish that value,
  so the owned value cannot be cleared by an outside client

#### Scenario: The component's own republish does not trigger a further republish
- **WHEN** the owning component republishes its own value in response to a detected divergence
- **THEN** receiving that same, matching value back SHALL NOT trigger another republish

#### Scenario: A renamed or removed key's old retained value is cleared
- **WHEN** a key that was previously exposed is no longer in the current version's exposed-key set,
  because it was renamed or removed, and its old value is still retained in the broker
- **THEN** the component SHALL publish an empty retained payload on that key's old topic, clearing it

#### Scenario: A demoted key's retained value is cleared the same way
- **WHEN** a setting's `exposable` marking is removed while the setting itself still exists
- **THEN** it SHALL be treated the same as a removed key: its old retained value SHALL be cleared

#### Scenario: A cleared topic does not trigger further clears
- **WHEN** the component receives an empty payload on a `config/<key>` topic whose expected state is
  absent
- **THEN** it SHALL take no action, so that a clear cannot echo into another clear

### Requirement: The agent serves exposed configuration over HTTP
The agent SHALL collect the retained configuration topics of every service (including its own) as an
ordinary MQTT subscriber,
and serve them as a read-only HTTP view:
`GET /te/v1/entities/<service-topic-id>/config` returns every exposed value for that service as a JSON
object mapping key to string value;
`GET /te/v1/entities/<service-topic-id>/config/<key>` returns a single value.

#### Scenario: Whole-service config is served as a JSON object
- **WHEN** a client sends `GET /te/v1/entities/device/main/service/tedge-mapper-c8y/config`
- **THEN** the response SHALL be a JSON object of every exposed key to its current value for that service

#### Scenario: A single exposed value is served
- **WHEN** a client sends `GET /te/v1/entities/device/main/service/tedge-agent/config/device.id`
- **THEN** the response SHALL contain that value

#### Scenario: The HTTP view does not accept writes
- **WHEN** a client sends `PUT`, `PATCH`, or `DELETE` to a `config` resource path
- **THEN** the agent SHALL reject the request;
  configuration can only be changed by its owning component

### Requirement: Unexposed and non-existent keys are indistinguishable
A request for a configuration key that is not marked exposable SHALL be treated identically to a
request for a key that does not exist at all,
so that the response reveals nothing about which non-exposed settings exist.

#### Scenario: Requesting a non-exposed key returns 404
- **WHEN** a client sends `GET /te/v1/entities/device/main/service/tedge-agent/config/device.key_pin`
- **THEN** the response SHALL be `404 Not Found`

#### Scenario: Requesting a key that does not exist returns the same 404
- **WHEN** a client sends `GET` for a configuration key that is not a real setting at all
- **THEN** the response SHALL also be `404 Not Found`, indistinguishable from the non-exposed-key case
