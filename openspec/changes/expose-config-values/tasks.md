## 1. Macro: exposable attribute

- [ ] 1.1 Add `exposable: bool` to `ConfigurableField` in `tedge_config_macros`
- [ ] 1.2 Thread `exposable` through field types and generate `ReadableKey::is_exposable()`
- [ ] 1.3 Add macro tests: exposable field, profiled exposable field, unmarked field

## 2. Allowlist marking and collection helpers

- [ ] 2.1 Mark the curated exposable set with `#[tedge_config(exposable)]` in `define_tedge_config!`
- [ ] 2.2 Add `exposed_core_config()` and `exposed_cloud_config()` helpers returning key-value pairs
- [ ] 2.3 Unit-test the helpers: core excludes cloud keys, cloud strips prefix, secrets never appear
- [ ] 2.4 Add forward-guarding test asserting known-secret keys are non-exposable

## 3. tedge_api: config channel and entity store

- [ ] 3.1 Add `Channel::Config { key }` with parse/serialize/`ChannelFilter`
- [ ] 3.2 Add `config` map to `EntityMetadata`, parallel to `twin_data`
- [ ] 3.3 Add entity-store ingestion accessors for config values
- [ ] 3.4 Unit-test channel round-tripping and entity-store accessors

## 4. Shared retained-config publisher

- [ ] 4.1 Add a shared publisher actor that publishes (key, optional value) pairs as retained messages
- [ ] 4.2 Subscribe to own `config/+` and reconcile against expected state
- [ ] 4.3 Unit-test publishing and tamper correction
- [ ] 4.4 Unit-test stale-key clearing

## 5. Agent and mapper integration

- [ ] 5.1 Compute and publish the agent's exposed core config at startup
- [ ] 5.2 Spawn the shared publisher for each mapper's cloud config
- [ ] 5.3 Subscribe the agent's entity store to config channel, ingest messages
- [ ] 5.4 Add HTTP routes: `GET .../config` and `GET .../config/<key>`
- [ ] 5.5 Test HTTP routes: JSON response, single value, 404, rejected writes

## 6. End-to-end tests and docs

- [ ] 6.1 Robot Framework suite for agent retained topics and HTTP routes
- [ ] 6.2 Extend suite for a bootstrapped c8y mapper
- [ ] 6.3 Document the config MQTT channel and HTTP routes
