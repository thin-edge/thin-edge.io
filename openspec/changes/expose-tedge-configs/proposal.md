## Why

An external process — a plugin, or a %%te%% component in its own container — has no access to the
config directory or the device certificate, and `tedge config get` needs both.
So even a non-secret, widely-needed value like `device.id` is out of reach for anything outside the
component that owns the config file.
There's currently no way to read a %%te%% config value without file access.

## What Changes

- Add an opt-in `exposable` marker to individual `tedge_config` settings, set where each setting is
  defined.
  A new setting stays hidden until a maintainer marks it — safe by default.
- Each owning component publishes its own exposable values as retained MQTT messages, one value per
  topic, under its own service topic: `te/device/<device>/service/<service>/config/<key>`.
  The agent publishes core/device settings;
  each cloud mapper publishes its own cloud settings under its own service topic, with the cloud (and
  profile) prefix stripped from the key (`c8y.url` → `.../config/url`).
  An empty retained payload means the value is unset or removed.
- The agent collects these topics (as an ordinary subscriber — it never reads another component's
  config file) and serves them over HTTP as a convenience view:
  `GET /te/v1/entities/<service>/config` (all exposed values as a JSON object) and
  `GET /te/v1/entities/<service>/config/<key>` (a single value).
  A key that isn't exposed and a key that doesn't exist both return `404 Not Found` — you can't tell
  them apart from outside.
- Curate the initial exposable set per the allowlist in this change's `design.md` (core device/MQTT/HTTP
  settings; per-cloud URL, device id, bridge topic prefix, feature-enable flags, topic lists; secrets and
  file paths excluded).
- The shared publisher also watches its own topics: if an external client overwrites a value, it
  republishes the correct one.

## Capabilities

### New Capabilities
- `config-exposure`: the opt-in allowlist mechanism, the retained per-value MQTT topic scheme, key
  naming and ownership rules, and the HTTP read view served by the agent.

## Impact

- `crates/common/tedge_config_macros` — new `#[tedge_config(exposable)]` field attribute and a
  generated `ReadableKey::is_exposable()`.
- `crates/common/tedge_config` — allowlist marking in `define_tedge_config!`, plus a helper to collect
  each component's exposable (key, value) pairs.
- `crates/core/tedge_api` — new `Channel::Config { key }` topic variant and matching `ChannelFilter`,
  plus config storage on the entity store (parallel to twin data).
- A new small extensions crate that publishes retained config messages at startup and corrects them if
  externally overwritten, used by both `crates/core/tedge_agent` and `crates/core/tedge_mapper`
  (c8y/aws/az).
- `crates/core/tedge_agent` — MQTT ingestion of `config/<key>` into the entity store, and new
  `GET /te/v1/entities/<service>/config[/<key>]` HTTP routes, modeled on the existing twin-data routes.
- `tests/RobotFramework` — end-to-end coverage of the retained topics and HTTP routes.
- `docs/` — MQTT topic reference and entity HTTP API reference gain the `config` channel/routes.
