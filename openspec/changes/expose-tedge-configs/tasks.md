> Commit per logical group of related tasks (e.g. a whole numbered section, or a cohesive subset of one
> that stands on its own), not per individual task item.

## 1. Macro: `exposable` attribute

- [x] 1.1 Add `#[darling(default)] exposable: bool` to `ConfigurableField` in `tedge_config_macros/impl/src/input/parse.rs`
- [x] 1.2 Thread `exposable` through `ReadOnlyField`/`ReadWriteField` and their accessor in `tedge_config_macros/impl/src/input/validate.rs`
- [x] 1.3 Add `exposable` to `ConfigurationKey` in `tedge_config_macros/impl/src/query.rs`, populate it in `enum_variant`, and generate `ReadableKey::is_exposable()` in `keys_enum`, cloning the `help()` pattern
- [x] 1.4 Add macro tests covering a plain exposable field, a profiled/multi-group exposable field, and an unmarked field

## 2. Allowlist marking and collection helper

- [x] 2.1 Mark every ✓ setting from the allowlist table in this change's `design.md` with `#[tedge_config(exposable)]` in `tedge_config`'s `define_tedge_config!`
- [x] 2.2 Add `exposed_core_config()` and `exposed_cloud_config(cloud, profile)` helpers (new `tedge_toml/config_exposure.rs`) returning key→value pairs, with `None` for unset exposable keys
- [x] 2.3 Unit-test the helpers: core excludes cloud keys; cloud helper strips the cloud/profile prefix; profile keys route to the right profile; secrets never appear; unset exposable keys yield `None`; the `c8y.entity_store.*` deprecated-alias keys are excluded (agent-owned, not mapper-owned)
- [x] 2.4 Add a forward-guarding test: a fixed list of known-secret keys (`device.key_pin`, `device.key_uri`, `cryptoki.pin`, `proxy.password`, credential/key file paths, per-cloud `device.key_*`) all report `is_exposable() == false`, so marking one exposable fails CI (no value masking exists; the allowlist is the only safeguard)

## 3. `tedge_api`: config channel and entity store

- [x] 3.1 Add `Channel::Config { key }` (parse/serialize/`ChannelFilter`) to `tedge_api/src/mqtt_topics.rs`, mirroring `EntityTwinData`
- [x] 3.2 Add a `config: BTreeMap<String, String>` map to `EntityMetadata`, parallel to `twin_data`, excluded from serialization
- [x] 3.3 Add entity-store accessors for MQTT ingestion, not an external write API: ingest/clear one config value, read one or all values, with the same key-validation as twin fragments. Name them as ingestion primitives (`ingest_config_value`/`clear_config_value`/`get_config`), not a mutation surface
- [x] 3.4 Unit-test channel round-tripping and the new entity-store accessors, including empty-payload-as-removal

## 4. Shared retained-config publisher

- [x] 4.1 Add a new extensions crate with a builder/actor that publishes a set of (key, optional value) pairs as retained messages under a given service topic, publishing an empty payload for unset values
- [x] 4.2 Subscribe the actor to its own service's `config/+` (one wildcard, not per-key) and reconcile each message against expected state (`Some(value)` for an exposed+set key, absent otherwise): payload ≠ expected value (including empty) → republish; expected absent + non-empty payload → clear; payload matches expected → no-op
- [x] 4.3 Unit-test the publisher against a captured MQTT message box
- [x] 4.4 Unit-test tamper correction: a divergent value on an owned key republishes; an empty payload on an owned+set key also republishes (empty must not wipe an owned value); the actor's own matching echo does not
- [x] 4.5 Unit-test stale-key clearing: a retained message for a key not in the exposed set (renamed/removed/demoted) is cleared; a key still in the set is untouched; an empty payload on an absent key does not clear (no clear-echo loop)

## 5. Agent publishes its own configuration

- [ ] 5.1 Compute the agent's exposed core config (including CLI-effective `mqtt.topic_root`/`mqtt.device_topic_id`) in `tedge_agent`'s startup config assembly
- [ ] 5.2 Spawn the shared publisher for the agent's service topic alongside the existing twin-manager actor

## 6. Mappers publish their own cloud configuration

- [ ] 6.1 Extend the mapper startup helper that already spawns the health monitor to also spawn the shared config publisher
- [ ] 6.2 Pass each mapper's own exposed cloud config (profile-aware) from the c8y, aws, and az mapper `build()` entry points

## 7. Agent aggregates retained config over MQTT

- [ ] 7.1 Subscribe the entity store to the new config channel filter
- [ ] 7.2 Ingest `config/<key>` messages into the entity store, treating an empty payload as removal
- [ ] 7.3 Clear retained config topics for a service when it is deregistered
- [ ] 7.4 Test ingestion, empty-payload removal, and deregistration cleanup

## 8. Agent HTTP read view

- [ ] 8.1 Add `GET /te/v1/entities/<service>/config` and `GET /te/v1/entities/<service>/config/<key>` routes, modeled on the existing twin-data routes
- [ ] 8.2 Return a JSON object for the whole-config view and a plain value for a single key; reject writes on `config` paths
- [ ] 8.3 Return `404 Not Found` for both non-exposed and non-existent keys, indistinguishably
- [ ] 8.4 Test both routes: object response, single-value response, 404 for a non-exposed key, 404 for an unknown entity, rejected write

## 9. End-to-end tests

- [ ] 9.1 Add a Robot Framework suite verifying the agent's retained config topics and HTTP routes, including that secret settings never appear
- [ ] 9.2 Extend the suite to cover a bootstrapped c8y mapper's retained config topics and HTTP routes

## 10. Documentation

- [ ] 10.1 Document the `config` MQTT channel in the MQTT topic reference
- [ ] 10.2 Document the two `config` HTTP routes in the entity HTTP API reference
