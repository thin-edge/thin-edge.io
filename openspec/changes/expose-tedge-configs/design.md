## Context

External processes cannot read thin-edge config values without filesystem access
and the device certificate — the [proposal](./proposal.md) covers the full problem
and motivating use cases.

Two constraints shape the design:

- **Ownership:** each config value has exactly one owner — the component that loads it.
  No component reads another's config file or publishes on another's behalf.
- **Safe by default:** config holds secrets (`device.key_pin`, credential-file paths),
  so values must be opted in to exposure, never opted out.

## Goals / Non-Goals

**Goals:**
- Read selected config values without file or certificate access —
  specifically the value the running process currently has loaded,
  refreshed only on that component's restart.
- Safe by default: a new setting is never exposed until explicitly marked.
- Respect ownership: no component reads or republishes another's config.
- Offer the same values via subscription-based MQTT and on-demand HTTP query.

**Non-Goals:**
- Live republish when `tedge.toml` changes without a restart —
  components load config at startup; a future dynamic-reload feature
  would republish after reload.
- Letting external clients write config values back through the HTTP or MQTT
  surface — the HTTP API is read-only, and config changes go through
  `tedge config set` plus a component restart.
- Exposing config values *from* child devices — this initial implementation
  covers services on the main device only.
  Child devices *reading* the main device's exposed config is a supported
  use case (they subscribe to MQTT or query HTTP like any other client).
- Broker ACLs / TLS for config topics — the self-correcting publisher handles
  casual overwrites; stronger access control is orthogonal.

## Decisions

### Opt-in exposure, not opt-out

Config holds secrets (`device.key_pin`, `cryptoki.pin`, credential-file paths).
With an allowlist, a new setting stays hidden until a maintainer marks it.
The worst case is a missing value, not a leaked secret.

Alternative: opt-out (denylist). Rejected — a new setting would be exposed
the moment it's added unless someone remembers to deny it.

### Each component publishes only what it owns

The agent publishes core/device settings (`tedge.toml`);
each mapper publishes only its own cloud settings (`mapper.toml`) under its own service topic.
No component reads another's config file.

Alternative: the agent reads all `mapper.toml` files and exposes everything centrally.
Rejected — couples the agent to every cloud's config shape and profiles,
breaking the rule that each setting's source of truth stays with its owner.

### Allowlist marking is a per-field macro attribute

`#[tedge_config(exposable)]` on each setting in `define_tedge_config!`,
generating `ReadableKey::is_exposable()`.
This follows the same pattern already used for `deprecated_key` and doc
comments — the decision to expose lives next to the setting definition.

### One retained JSON document per service, not per-value topics

Each service publishes a single retained MQTT message on
`te/device/<device>/service/<service>/config`, containing every exposed
key-value pair as one JSON object. Each field name inside that object is the
`tedge config` key kept flat (e.g: `device.id`), so it maps straight back to
a `tedge config` key with no extra parsing.

A consumer subscribes to one topic and gets the service's whole exposed
config; there's no way to subscribe to a single key over MQTT — a client
that wants just one value uses the HTTP API instead.

Alternative: one retained message per key (`config/<key>`). Rejected —
publishing and reconciling N per-key topics is proportionally more MQTT
traffic and state to track for no benefit over parsing one JSON object.
It also complicates clearing a stale key after a rename or version
upgrade (each one needs its own explicit empty-payload clear — see
"Self-correcting publisher" below, where the aggregate document avoids this
entirely).

### Mapper-published keys drop the cloud and profile qualifier

`c8y.url` becomes the `url` field in `.../tedge-mapper-c8y/config`'s JSON
object.
The service topic already scopes the value; repeating the cloud/profile
prefix in the field name would be redundant.
That name comes from `bridge.topic_prefix`: `format!("tedge-mapper-{prefix}")`

### One shared publisher

At startup, a component publishes one retained message: a JSON object of
every currently-set exposed key-value pair. Unset exposable keys are simply
omitted from the object.
Because the whole document is replaced as a unit, a value removed from config, 
or a key no longer marked exposable, can't linger from a previous run:
the next publish just doesn't include it.

### Self-correcting publisher

Each component subscribes to its own `config` topic and compares the
retained payload against its expected JSON object:

- Payload doesn't match the expected object (wrong content, or absent) —
  republish the expected object.
- Payload matches — no-op (terminates the loop).

This corrects external overwrites within one round trip. Because the
document is replaced as a whole rather than patched key by key, this same
comparison also clears stale keys left behind by renames, demotions, or
version upgrades — they're simply absent from the republished object, with
no separate clearing path to implement or test.

### Config stored parallel to twin data, not merged into it

The entity store gets a `config` map on each entity, separate from
`twin_data`. Config values come from one source of truth (the owning
component's `tedge_config`) and nothing external can set them.
Merging into twin data would blur that distinction.

### Unexposed and non-existent keys are indistinguishable

The agent collects config purely by subscribing to each service's `config`
topic and parsing the retained JSON object — it never knows which keys exist
but are hidden vs. which don't exist at all.
Both the MQTT view (no retained message) and the HTTP API (`404 Not Found`)
treat the two cases identically, so the response leaks nothing about which
non-exposed settings exist.

## Initial exposable set

The curated set (✓ = exposed, ✗ = not exposed):

| Config key | Expose |
|---|:--:|
| `agent.enable.config_snapshot` | ✓ |
| `agent.enable.config_update` | ✓ |
| `agent.enable.log_upload` | ✓ |
| `agent.entity_store.auto_register` | ✓ |
| `agent.entity_store.clean_start` | ✗ |
| `agent.state.path` | ✗ |
| `apt.dpkg.options.config` | ✗ |
| `apt.maintainer` | ✗ |
| `apt.name` | ✗ |
| `aws.bridge.keepalive_interval` | ✗ |
| `aws.bridge.topic_prefix` | ✓ |
| `aws.device.cert_path` | ✗ |
| `aws.device.csr_path` | ✗ |
| `aws.device.id` | ✓ |
| `aws.device.key_path` | ✗ |
| `aws.device.key_pin` | ✗ |
| `aws.device.key_uri` | ✗ |
| `aws.mapper.mqtt.max_payload_size` | ✓ |
| `aws.mapper.timestamp` | ✗ |
| `aws.mapper.timestamp_format` | ✗ |
| `aws.root_cert_path` | ✗ |
| `aws.topics` | ✓ |
| `aws.url` | ✓ |
| `az.bridge.keepalive_interval` | ✗ |
| `az.bridge.topic_prefix` | ✓ |
| `az.device.cert_path` | ✗ |
| `az.device.csr_path` | ✗ |
| `az.device.id` | ✓ |
| `az.device.key_path` | ✗ |
| `az.device.key_pin` | ✗ |
| `az.device.key_uri` | ✗ |
| `az.mapper.mqtt.max_payload_size` | ✗ |
| `az.mapper.timestamp` | ✗ |
| `az.mapper.timestamp_format` | ✗ |
| `az.root_cert_path` | ✗ |
| `az.topics` | ✓ |
| `az.url` | ✓ |
| `c8y.auth_method` | ✓ |
| `c8y.availability.enable` | ✗ |
| `c8y.availability.interval` | ✗ |
| `c8y.bridge.include.local_cleansession` | ✗ |
| `c8y.bridge.keepalive_interval` | ✗ |
| `c8y.bridge.topic_prefix` | ✓ |
| `c8y.credentials_path` | ✗ |
| `c8y.device.cert_path` | ✗ |
| `c8y.device.csr_path` | ✗ |
| `c8y.device.id` | ✓ |
| `c8y.device.key_path` | ✗ |
| `c8y.device.key_pin` | ✗ |
| `c8y.device.key_uri` | ✗ |
| `c8y.enable.config_snapshot` | ✓ |
| `c8y.enable.config_update` | ✓ |
| `c8y.enable.device_profile` | ✓ |
| `c8y.enable.device_restart` | ✓ |
| `c8y.enable.firmware_update` | ✓ |
| `c8y.enable.log_upload` | ✓ |
| `c8y.enable.software_update` | ✓ |
| `c8y.entity_store.auto_register` | ✗ |
| `c8y.entity_store.clean_start` | ✗ |
| `c8y.http` | ✓ |
| `c8y.mapper.mqtt.max_payload_size` | ✓ |
| `c8y.mqtt` | ✓ |
| `c8y.mqtt_service.enabled` | ✓ |
| `c8y.mqtt_service.topics` | ✓ |
| `c8y.operations.auto_log_upload` | ✗ |
| `c8y.proxy.bind.address` | ✗ |
| `c8y.proxy.bind.port` | ✗ |
| `c8y.proxy.ca_path` | ✗ |
| `c8y.proxy.cert_path` | ✗ |
| `c8y.proxy.client.host` | ✓ |
| `c8y.proxy.client.port` | ✓ |
| `c8y.proxy.key_path` | ✗ |
| `c8y.root_cert_path` | ✗ |
| `c8y.smartrest.child_device.create_with_device_marker` | ✗ |
| `c8y.smartrest.templates` | ✓ |
| `c8y.smartrest.use_operation_id` | ✗ |
| `c8y.smartrest1.templates` | ✓ |
| `c8y.software_management.api` | ✗ |
| `c8y.software_management.with_types` | ✗ |
| `c8y.topics` | ✓ |
| `c8y.url` | ✓ |
| `certificate.organization` | ✗ |
| `certificate.organization_unit` | ✗ |
| `certificate.validity.minimum_duration` | ✗ |
| `certificate.validity.requested_duration` | ✗ |
| `configuration.plugin_paths` | ✗ |
| `data.path` | ✗ |
| `device.cert_path` | ✗ |
| `device.cryptoki.mode` | ✗ |
| `device.cryptoki.module_path` | ✗ |
| `device.cryptoki.pin` | ✗ |
| `device.cryptoki.socket_path` | ✗ |
| `device.cryptoki.uri` | ✗ |
| `device.csr_path` | ✗ |
| `device.id` | ✓ |
| `device.key_path` | ✗ |
| `device.key_pin` | ✗ |
| `device.key_uri` | ✗ |
| `device.type` | ✓ |
| `diag.plugin_paths` | ✗ |
| `firmware.child.update.timeout` | ✗ |
| `flows.memory.heap_size` | ✗ |
| `flows.memory.stack_size` | ✗ |
| `flows.params.keep_on_delete` | ✗ |
| `flows.stats.interval` | ✗ |
| `flows.stats.on_interval` | ✗ |
| `flows.stats.on_message` | ✗ |
| `flows.stats.on_startup` | ✗ |
| `http.bind.address` | ✗ |
| `http.bind.port` | ✗ |
| `http.ca_path` | ✗ |
| `http.cert_path` | ✗ |
| `http.client.auth.cert_file` | ✗ |
| `http.client.auth.key_file` | ✗ |
| `http.client.host` | ✓ |
| `http.client.port` | ✓ |
| `http.key_path` | ✗ |
| `log.plugin_paths` | ✗ |
| `logs.max_per_operation` | ✗ |
| `logs.path` | ✗ |
| `mqtt.bind.address` | ✗ |
| `mqtt.bind.enabled` | ✗ |
| `mqtt.bind.port` | ✗ |
| `mqtt.bridge.built_in` | ✗ |
| `mqtt.bridge.reconnect_policy.initial_interval` | ✗ |
| `mqtt.bridge.reconnect_policy.maximum_interval` | ✗ |
| `mqtt.bridge.reconnect_policy.reset_window` | ✗ |
| `mqtt.client.auth.ca_dir` | ✗ |
| `mqtt.client.auth.ca_file` | ✗ |
| `mqtt.client.auth.cert_file` | ✗ |
| `mqtt.client.auth.key_file` | ✗ |
| `mqtt.client.auth.password_file` | ✗ |
| `mqtt.client.auth.username` | ✗ |
| `mqtt.client.host` | ✓ |
| `mqtt.client.port` | ✓ |
| `mqtt.device_topic_id` | ✓ |
| `mqtt.external.bind.address` | ✗ |
| `mqtt.external.bind.interface` | ✗ |
| `mqtt.external.bind.port` | ✗ |
| `mqtt.external.ca_path` | ✗ |
| `mqtt.external.cert_file` | ✗ |
| `mqtt.external.key_file` | ✗ |
| `mqtt.topic_root` | ✓ |
| `proxy.address` | ✗ |
| `proxy.no_proxy` | ✗ |
| `proxy.password` | ✗ |
| `proxy.username` | ✗ |
| `run.lock_files` | ✗ |
| `run.log_memory_interval` | ✗ |
| `run.path` | ✗ |
| `service.timestamp_format` | ✗ |
| `service.type` | ✗ |
| `software.plugin.default` | ✗ |
| `software.plugin.exclude` | ✗ |
| `software.plugin.include` | ✗ |
| `software.plugin.max_packages` | ✗ |
| `sudo.enable` | ✗ |
| `tmp.path` | ✗ |

HTTP and MQTT share the same per-service scoping and the same keys:

```
te/device/main/service/tedge-agent/config
  -> {"device.id":"my-device-01","mqtt.client.port":"1883","http.client.port":"8000"}
te/device/main/service/tedge-mapper-c8y/config
  -> {"url":"example.cumulocity.com","device.id":"my-device-01"}
te/device/main/service/tedge-mapper-c8y-edge/config
  -> {"url":"edge.c8y.io"}
```

The HTTP single-value route (`GET .../config/<key>`) looks the key up in the
same parsed object — it isn't backed by a separate MQTT topic per key.

## Risks / Trade-offs

- The allowlist is the only safeguard against leaking a secret — there is no
  value masking or secret newtype.
  A forward-guarding CI test asserts known-secret keys are non-exposable,
  so marking one fails CI.
- A removed mapper profile's config topic can only be cleared when that
  profile's entity is explicitly deregistered — the same gap twin data has
  for a decommissioned profile.
- A service's config only appears once its owner has published its document
  at least once. Consumers that need a value before the owning component
  starts must wait on MQTT for the retained message to arrive.
