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

The agent publishes core/device settings;
each mapper publishes only its own cloud settings under its own service topic.
No component reads another's config file.

Alternative: the agent reads all `mapper.toml` files and exposes everything
centrally. Rejected — couples the agent to every cloud's config shape and
profiles, breaking the rule that each setting's source of truth stays with
its owner.

### Allowlist marking is a per-field macro attribute

`#[tedge_config(exposable)]` on each setting in `define_tedge_config!`,
generating `ReadableKey::is_exposable()`.
This follows the same pattern already used for `deprecated_key` and doc
comments — the decision to expose lives next to the setting definition.

### Per-value retained topics, not one aggregate document

Each value gets its own retained MQTT message:
`te/device/<device>/service/<service>/config/<key>`.
`<key>` is the `tedge config` key kept as a single final topic segment
(not split into `config/device/id`), so it maps straight back to a
`tedge config` key with no extra parsing.

A consumer subscribes to exactly the value(s) it needs.
An empty retained payload means the value is unset or was removed.

Alternative: splitting the key across topic segments. Rejected — variable
depth and extra logic to reconstruct the key.

### Mapper-published keys drop the cloud and profile qualifier

`c8y.url` becomes `.../tedge-mapper-c8y/config/url`.
The service topic already scopes the value; repeating the cloud/profile
prefix in the key would be redundant.

### Self-correcting publisher via wildcard subscription

Each component subscribes to its own `config/+` and reconciles every
message against expected state (`Some(value)` for an exposed+set key,
absent for everything else):

- Payload differs from expected value (including an empty payload on an
  owned key) — republish the expected value.
- Expected absent but payload is non-empty — clear with an empty retained
  payload.
- Payload matches expected state — no-op (terminates the loop).

This does two jobs: corrects external overwrites within one round trip,
and clears stale keys left behind by renames or upgrades (their expected
state is absent, so the replayed retained message gets cleared).

### Config stored parallel to twin data, not merged into it

The entity store gets a `config` map on each entity, separate from
`twin_data`. Config values come from one source of truth (the owning
component's `tedge_config`) and nothing external can set them.
Merging into twin data would blur that distinction.

### Unexposed and non-existent keys are indistinguishable

The agent collects config purely by subscribing to MQTT — it never knows
which keys exist but are hidden vs. which don't exist at all.
Both the MQTT view (no retained message) and the HTTP API (`404 Not Found`)
treat the two cases identically, so the response leaks nothing about which
non-exposed settings exist.

## Risks / Trade-offs

- The allowlist is the only safeguard against leaking a secret — there is no
  value masking or secret newtype.
  A forward-guarding CI test asserts known-secret keys are non-exposable,
  so marking one fails CI.
- A removed mapper profile's config topics can only be cleared when that
  profile's entity is explicitly deregistered — the same gap twin data has
  for a decommissioned profile.
- A value only appears once its owner has published it at least once.
  Consumers that need a value before the owning component starts must wait
  on MQTT for the retained message to arrive.
