## Why

External processes — plugins, containerized components, child devices —
cannot read thin-edge config values like `device.id` without filesystem access
to `/etc/tedge` and the device certificate.
The Cumulocity opcua-device-gateway, for example, needs `device.id` for its
Cumulocity external ID and `c8y.http` for API calls;
today it achieves this by mounting `/etc/tedge` and running `tedge config get`,
which also requires the device certificate to be present.

## Proposed solution

Read a selected config value without any file or certificate access, either way:

- **Subscribe** to its retained MQTT topic, scoped to the owning service:

  ```
  te/device/main/service/tedge-agent/config/device.id       -> my-device-01
  te/device/main/service/tedge-mapper-c8y/config/url         -> example.cumulocity.com
  ```

- **Query** it over HTTP from the agent — a single value, or a whole service's
  config as a JSON object:

  ```console
  $ curl http://tedge:8000/te/v1/entities/device/main/service/tedge-agent/config/device.id
  my-device-01

  $ curl http://tedge:8000/te/v1/entities/device/main/service/tedge-mapper-c8y/config
  {"url":"example.cumulocity.com","device.id":"my-device-01"}
  ```

Only values a component maintainer has explicitly marked as exposable are ever
published or served.
The set never includes secrets (private keys, PINs, credential-file paths).

## What Changes

- Each owning component publishes its exposable config values as retained MQTT
  messages, one per topic: `te/device/<device>/service/<service>/config/<key>`.
  The agent publishes core/device settings;
  each mapper publishes its own cloud settings with the cloud/profile prefix
  stripped from the key.
- The agent subscribes to every service's `config/+` topics and serves them as a
  read-only HTTP view:
  `GET .../config` (JSON object) and `GET .../config/<key>` (single value).
  A key that isn't exposed and a key that doesn't exist both return `404`.
- The exposed set is opt-in per setting — a new setting stays hidden until a
  maintainer marks it.
- Each publisher self-corrects by subscribing to its own `config/+` topics and
  reconciling every message against expected state, correcting external overwrites
  and clearing stale keys from prior versions.
- Exposed values reflect the active/applied config (what the running process has
  loaded), not a live mirror of the file on disk.

## Capabilities

### New Capabilities
- `config-exposure`: opt-in allowlist, retained per-key MQTT topics, key-naming
  and ownership rules, and the read-only HTTP view served by the agent

### Modified Capabilities

## Impact

- `tedge_config_macros` — new `#[tedge_config(exposable)]` field attribute,
  generates `ReadableKey::is_exposable()`
- `tedge_config` — allowlist marking in `define_tedge_config!`,
  `exposed_core_config()` / `exposed_cloud_config()` helpers
- `tedge_api` — new `Channel::Config { key }` topic variant,
  config storage on the entity store
- New shared publisher crate used by the agent and all three mappers
- `tedge_agent` — MQTT ingestion of config topics, new HTTP routes
- Robot Framework end-to-end tests
- Docs: MQTT topic reference and HTTP API reference
