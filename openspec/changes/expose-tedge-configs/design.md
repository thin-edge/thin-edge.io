## Context

%%te%% config lives in files (`tedge.toml`, per-mapper `mapper.toml`) read only by the components that
own them.
An external process — a plugin, or a %%te%% component in its own container — has no access to the
config directory or the device certificate, since these aren't mounted into other containers, and
`tedge config get` needs both.
So even a non-secret, widely-needed value like `device.id` is out of reach for anything outside the
component that owns the config file.

## Goals / Non-Goals

**Goals:**
- Read selected config values without file or certificate access — specifically, the value the
  running process currently has loaded, refreshed only on that process's restart, not a live mirror of
  the file on disk.
- Safe by default: a new setting is never exposed until explicitly marked, and secrets are never
  exposed by accident.
- Respect ownership: no component reads or republishes another component's config.
- Support both push (retained MQTT) and pull (HTTP) for the same values.

**Non-Goals:**
- Live republish when `tedge.toml` changes without a restart — components already only load config at
  startup, and a full restart is the existing way to pick up new config.
  A future dynamic-reload feature would keep the same "reflects what's loaded" rule by republishing
  after reload, not by watching `tedge.toml`.
- Writing config values externally — the `config` HTTP surface is read-only; only the owning
  component can change its own values, via `tedge config set` and a restart.
- Persisting config values across a broker restart via the agent's own storage — retained MQTT already
  does this, and republishing at startup is cheap.

## Decisions

### Opt-in exposure, not opt-out

Config holds secrets (`device.key_pin`, `cryptoki.pin`, credential-file paths), so the exposed set
needs curation — either an allowlist (opt-in) or a denylist (opt-out).
We pick opt-in.
With a denylist, a new setting is exposed the moment it's added, and it leaks unless
someone remembers to deny it.
With an allowlist, a new setting stays hidden until a maintainer marks it — the worst case is a missing
value, not a leaked secret.

**Alternative considered**

Opt-out (denylist). Rejected for the reason above.

### Ownership: each component exposes only what it owns

The agent exposes core/device settings (`device.id`, the local MQTT/HTTP endpoints); each mapper
exposes only its own cloud's settings for its own profile (`c8y.device.id`, `c8y.mqtt`, `az.url`).
No component reads another's config file to expose it on that component's behalf.

**Alternative considered**

The agent reads all `mapper.toml` files and exposes everything centrally.
Rejected — it couples the agent to every cloud's config shape and profiles,
breaking the rule that each setting's source of truth stays with its owner.

### Allowlist marking is a per-field macro attribute, not a group-level or file-level flag

`#[tedge_config(exposable)]` goes directly on each setting in `define_tedge_config!`,
generating a `ReadableKey::is_exposable()` accessor — the same pattern already used for
`deprecated_key`/doc comments.
Marking exposability where the setting is defined keeps the decision reviewable next to the value
itself.

### Per-value retained topics, not one aggregate document per component

Each owner publishes each exposable value as its own retained MQTT message, one per topic, under its
own service topic:

```
te/device/<device>/service/<service>/config/<key>
```

`<key>` is the config key as `tedge config` uses it, kept as a single final topic segment (not split
into `config/device/id`), so it maps straight back to a `tedge config` key with no extra parsing.
This matches the existing per-fragment twin-data model, lets a consumer subscribe to exactly the
value(s) it needs, and gives clean per-value updates (empty retained payload = removal, see below).

Alternative considered: one aggregate config topic per component instead of one topic per value.
Rejected — it forces every consumer to fetch and parse the whole document just to read one value.

**Alternative considered**

Splitting the key across topic segments (`config/device/id` instead of
`config/device.id`). Rejected — the key would then have variable depth and need extra logic to map
back to a `tedge config` key.

### Mapper-published keys drop the cloud (and profile) qualifier

A mapper publishes under its own service topic, so `c8y.url` becomes `.../tedge-mapper-c8y/config/url`.
The service topic already scopes the value; repeating the cloud/profile prefix in the key is redundant.

The publisher reuses the mapper's existing service topic; it does not build a new name.
That name comes from `bridge.topic_prefix`, not the profile: `format!("tedge-mapper-{prefix}")`
(`tedge_mapper/src/c8y/mapper.rs`).
A profile is `tedge-mapper-c8y-edge` only when its `bridge.topic_prefix` is `c8y-edge`.
Examples below assume that convention.

Stripping the qualifier off the key is mechanical for ordinary cloud keys (`c8y.<key>`,
`c8y.profiles.<name>.<key>`).
The helper skips the `c8y.entity_store.*` deprecated-alias keys (see "Deprecated aliases…").

### One shared publisher, one shared collection helper

The (key, value) collection for "everything this component is allowed to expose" is a pure function in
`tedge_config` (`exposed_core_config()` for the agent, `exposed_cloud_config(cloud, profile)` for a
mapper), built on the existing `readable_keys()` / `read_string()` used by `tedge config list`.
The publishing itself (topic construction, retain flag, the actor) is one small shared actor used by
the agent and all three mappers, instead of three near-duplicate implementations.

Caveat: some values are overridden at launch by a CLI flag or env var not written back to
`tedge_config`.
The agent reads `mqtt.topic_root` and `mqtt.device_topic_id` as `cliopts.<x>.unwrap_or(tedge_config.<x>)`
(`tedge_agent/src/agent.rs`).
For these, `read_string()` returns the file value, not what the process runs with.
The helper stays pure; the agent patches these CLI-effective entries after computing the base set.

### Unset exposable values are actively cleared, not just omitted

At startup, a component publishes every exposable key in its scope: the value if set, or an empty
payload (which clears the retained message) if unset.
This means a value removed from config can't linger from a previous run — twin-data already relies on
the same behavior.

One consequence: to any subscriber connecting later — including the agent's own aggregating
subscription — an exposable-but-unset key and a never-exposed key look identical: no retained message
at all.
See "Unexposed and non-existent keys are indistinguishable" below.

### The shared publisher subscribes to its own `config/+` and reconciles every message it sees

The publisher subscribes to its whole config namespace with one wildcard,
`te/device/<device>/service/<service>/config/+`, not to individual keys.
It holds an *expected state* per topic: `Some(value)` for an exposed, set key; *absent* for everything
else (an exposed key that is unset, or any key not in its exposed set).
One handler reconciles every message — replayed or later — to that state:

- expected `Some(v)`, payload ≠ `v` (including empty) → republish `v`.
- expected absent, payload non-empty → clear with an empty payload.
- payload already matches expected state → no-op.

The no-op case is the only one, and it is the actor's own echo, so the loop terminates.
Keying off expected state (not special-casing empty payloads) makes this loop-safe and complete:
an empty payload is ignored only when the topic should be empty, never when it would wipe an owned value.

One subscription does two jobs:

**Tamper defense.** Any local client can overwrite a retained config message.
Self-correction closes the window to one round trip instead of until restart.
Broker ACLs and the agent's TLS/client-cert options are the stronger fix, out of scope here.

**Clearing stale keys across upgrades.**
A key renamed with no `deprecated_key` alias, or removed, leaves no variant for the startup pass to
publish, and `tedge_config` keeps no list of removed keys.
Its old value would otherwise linger forever.
The wildcard replays these orphaned messages on connect; their expected state is absent, so the handler
clears them.
A demoted key (marker removed, setting kept) clears the same way — its expected state is also absent.

The startup publish pass and this reconciliation are both required.
The pass publishes keys the binary knows about, including a new key with no retained message yet, which
reconciliation could never publish since it only reacts to received messages.
Reconciliation covers what the pass cannot see: orphaned keys and later tampering.

**Restart transient.**
After a config change, the broker still holds old values.
On startup the component publishes the new values and, via the subscription, republishes over each
replayed old value — so a changed key is published about twice before settling.
Bounded, self-terminating, and proportional to the number of changed keys (zero when nothing changed).

Alternatives considered:
- A one-shot query of the retained-message cache at startup (the `retain_requests` channel
  `deregister_entity` uses in `tedge_mqtt_ext`) to find, clear, and diff-publish only deltas.
  Removes the restart transient too.
  Rejected — the wildcard subscription already delivers those messages, so a second channel buys nothing
  but saving a few free publishes.
  (Deregistration needs the query because it has no subscription to the topics it clears; this actor
  does.)
- A hand-maintained list of removed key names, like `version.rs`'s TOML migrations.
  Rejected — reacting to actual retained state needs no history.

### Config values are stored and served like twin data, not merged into it

The entity store gets a `config` map on each entity, parallel to (not merged with) `twin_data`: config
values come from one source of truth (the owning component's `tedge_config`) and nothing external can
set them, whereas twin data is an arbitrary JSON document any authorized client can write.
Reusing the twin storage would blur that distinction.

### Unexposed and non-existent keys are indistinguishable

Neither the retained topic nor the HTTP `404` gives any signal about whether a key is a real,
non-exposed setting or doesn't exist at all — on purpose, so the boundary between "exists but secret"
and "doesn't exist" leaks nothing.
This isn't a special rule bolted onto the HTTP layer — the agent has no other source of truth for a key
besides what arrives over MQTT.
It collects every service's config purely by subscribing, including its own, so it can never know more
than "did a retained message arrive for this key."

### Deprecated aliases are never exposed; ownership decides who publishes

A deprecated key is never marked exposable in its own right.
Ownership already picks the right owner, so the alias adds nothing.

Case in point: `entity_store.auto_register`/`clean_start`.
`c8y.entity_store.auto_register` exists as a legacy group and as the `deprecated_key` alias for
`agent.entity_store.auto_register`.
But the agent owns and reads it (`tedge_agent/src/agent.rs`); no mapper reads the `c8y.entity_store.*`
form.
So the agent exposes `agent.entity_store.auto_register` (✓) and the c8y mapper marks
`c8y.entity_store.*` unexposed (✗) — because it does not own them, not because they are aliases.

Every other `deprecated_key` (`mqtt.port`, `http.port`, `mqtt.external.*`) resolves to an unexposed
setting, so none need exposing.

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
te/device/main/service/tedge-agent/config/device.id            -> "my-device-01"
te/device/main/service/tedge-agent/config/mqtt.client.port     -> "1883"
te/device/main/service/tedge-agent/config/http.client.port     -> "8000"
te/device/main/service/tedge-mapper-c8y/config/url             -> "example.cumulocity.com"
te/device/main/service/tedge-mapper-c8y/config/device.id       -> "my-device-01"
te/device/main/service/tedge-mapper-c8y-edge/config/url        -> "edge.c8y.io"
```

```console
$ curl http://tedge:8000/te/v1/entities/device/main/service/tedge-agent/config/device.id
my-device-01

$ curl http://tedge:8000/te/v1/entities/device/main/service/tedge-mapper-c8y/config
{"url":"example.cumulocity.com","device.id":"my-device-01"}

$ curl -s -o /dev/null -w '%{http_code}' \
    http://tedge:8000/te/v1/entities/device/main/service/tedge-agent/config/device.key_pin
404
```

## Risks / Trade-offs

- **The allowlist is the only safeguard against leaking a secret; there is no value masking.**
  `tedge_config` stores secrets as plain types (`proxy.password: String`, `device.key_pin: Arc<str>`),
  `read_string()` renders them verbatim, and `tedge config list` already prints `proxy.password` in
  cleartext.
  A single mis-marking would publish plaintext to a retained topic and the HTTP view — a wider blast
  radius than the root-readable file.
  Mitigation: a forward-guarding test asserts known-secret keys are non-exposable, so a future edit that
  flips one fails CI.
  Out-of-scope follow-up: a secret newtype whose `Display` masks and that `#[tedge_config(exposable)]`
  refuses to compile on — this would also fix the existing `tedge config list` leak.
- A config key whose owning mapper *profile* is removed from `tedge.toml` entirely (not just a setting
  within it) can't be cleared by the reconciliation above, since no process ever starts under
  that service topic again to run it — clearing it depends on that profile's entity being explicitly
  deregistered, the same gap twin data has for a decommissioned profile.
- Retained config messages can be overwritten by any local client with broker access; the owning
  component's self-correction (see Decisions) closes this within one round trip rather than leaving it
  until restart, but broker ACLs and the agent's existing TLS/client-cert options remain the stronger
  guarantee and are out of scope here.
- The HTTP view is read-only, and there's no way to ask a mapper that hasn't started yet for a value
  directly — a value only shows up once its owner has published it at least once.

## Capabilities

### New Capabilities
- `config-exposure`
