## 1. Bridge Template System Extension

- [x] 1.1 Add `mapper_config: Option<&toml::Table>` field to `TemplateContext` struct
- [x] 1.2 Register `mapper` as a recognised namespace keyword in the template parser (alongside `config` and `connection`)
- [x] 1.3 Implement `${mapper.KEY}` resolution by walking the TOML table (support nested keys like `${mapper.bridge.topic_prefix}`)
- [x] 1.4 Return a clear error when `${mapper.KEY}` is used but the key does not exist in the table
- [x] 1.5 Add unit tests for `${mapper.*}` expansion (simple key, nested key, missing key)
- [x] 1.6 Add unit test for combining `${mapper.*}` and `${config.*}` in the same template

## 2. Custom Mapper Config Parsing

- [x] 2.1 Redefine `CustomMapperConfig`: remove the `[connection]` TOML wrapper section; move `url` to a top-level key deserialized as `HostPort` (combining host and optional port, matching built-in mapper URL convention); remove the separate `port` field
- [x] 2.2 Update the `tedge.toml` parser and all call sites to use the new top-level `url: HostPort` field; update unit tests to use the new format
- [x] 2.3 Emit a clear parse error including the file path and TOML error location when the file contains invalid TOML
- [x] 2.4 Add unit tests for config parsing: valid TOML, invalid TOML, file not found

## 3. CLI Subcommand

- [x] 3.1 Add `Custom { profile: Option<ProfileName> }` variant to the `MapperName` enum
- [x] 3.2 Configure the clap subcommand: `tedge-mapper custom --profile <name>` with `--profile` as an optional long argument
- [x] 3.3 Implement profile validation: check that the `custom.{name}/` directory exists; if not, emit an error listing available `custom.*` profiles
- [x] 3.4 Add unit tests for profile validation (profile exists, profile missing, no profile provided)

## 4. CustomMapper Component

- [x] 4.1 Create `CustomMapper` struct with `profile: Option<ProfileName>` field
- [x] 4.2 Implement directory resolution: derive the mapper directory path (`custom.{name}/` or `custom/`) from the profile
- [x] 4.3 Implement startup: check which of `tedge.toml`, `bridge/`, and `flows/` are present
- [x] 4.4 Return a clear error when `bridge/` is present but `tedge.toml` is absent
- [x] 4.5 If `tedge.toml` is present: parse config, extract connection/TLS details, start the MQTT bridge with the mapper config table passed as `${mapper.*}` context
- [x] 4.6 If `flows/` is present: start the flows engine, loading flows from that directory
- [x] 4.7 Error when mapper directory contains neither `tedge.toml` nor `flows/`: remove the empty-directory warn-and-continue logic and replace with a clear error message indicating the mapper would do nothing

## 5. Service Identity

- [x] 5.1 Derive the service name as `tedge-mapper-custom@{profile}` (with profile) or `tedge-mapper-custom` (without)
- [x] 5.2 Derive the health topic as `te/device/main/service/{service-name}/status/health`
- [x] 5.3 Use the lock file path `/run/{service-name}.lock` to prevent duplicate instances
- [x] 5.4 Derive the bridge service name as `tedge-mapper-bridge-custom@{profile}` or `tedge-mapper-bridge-custom`
- [x] 5.5 Add unit tests verifying the derived names for both profiled and unprofiled cases

## 6. Unrecognised Directory Warnings

- [x] 6.1 Implement a function that scans `/etc/tedge/mappers/` and classifies each directory as built-in, profile of built-in, custom, or unrecognised
- [x] 6.2 Emit a warning log for each directory that is not in a recognised class
- [x] 6.3 Call the scan function on mapper startup
- [x] 6.4 Add unit tests: recognised directories produce no warnings; typo in custom name produces a warning; completely unknown name produces a warning

## 1a. Bridge Template `${mapper.*}` Scope Tests

These tests live in `mod.rs` (the `expand()` integration level) and verify `${mapper.*}` behaviour end-to-end, complementing the unit tests in `template.rs`.

- [x] 1a.1 Test: `${mapper.*}` in `local_prefix` expands correctly when `mapper_config` is passed to `expand()` (integration test using a full `PersistedBridgeConfig`)
- [x] 1a.2 Test: `${mapper.*}` in a `[[rule]]` `topic` expands correctly end-to-end
- [x] 1a.3 Test: `${mapper.*}` in a `[[template_rule]]` `topic` (combined with `${item}`) expands correctly end-to-end
- [x] 1a.4 Test: `if = "${mapper.some_flag}"` produces a parse error with the span pointing to `mapper` (unit test: `mapper_namespace_rejected_in_if_condition`)
- [x] 1a.5 Test: `for = "${mapper.some_list}"` produces a parse error with the span pointing to `mapper` and the message indicating the `for` field failed (unit test: `mapper_namespace_rejected_in_for_loop`)

## 7. Integration Tests

- [ ] 7.1 Test: flows-only custom mapper starts successfully and processes messages (Robot Framework / system test)
- [ ] 7.2 Test: custom mapper with `tedge.toml` and `bridge/` establishes the MQTT bridge (Robot Framework / system test)
- [ ] 7.3 Test: custom mapper with all three components (`tedge.toml`, `bridge/`, `flows/`) starts all subsystems (Robot Framework / system test)
- [x] 7.4 Test: `bridge/` without `tedge.toml` produces a clear error (unit test: `startup_config::bridge_dir_without_tedge_toml_errors`)
- [x] 7.5 Test: `tedge-mapper custom --profile nonexistent` produces an error listing available profiles (unit test: `profile_validation::errors_when_directory_missing`, `error_lists_available_profiles`)
- [ ] 7.6 Test: two custom mapper profiles can run concurrently without interfering (Robot Framework / system test)
- [x] 7.7 Test: `tedge config list` does not include any custom mapper settings (structural: custom mapper config is not in `define_tedge_config!`, it reads from mapper-local `tedge.toml` only)

## 8. Documentation

- [ ] 8.1 Document the custom mapper directory layout (`custom.{name}/`, `tedge.toml`, `bridge/`, `flows/`)
- [ ] 8.2 Document the `tedge.toml` schema (required fields for bridge, optional extra fields, available in `${mapper.*}`)
- [ ] 8.3 Document the `${mapper.*}` template namespace with examples alongside `${config.*}`
- [ ] 8.4 Write a ThingsBoard walkthrough example (directory layout, `tedge.toml`, bridge rules, flow scripts, systemd unit)

## 9. Code Quality / Cleanup

- [x] 9.1 Remove inline task-reference comments from `mapper.rs` (e.g. `// 3.3:`, `// 4.3/4.4:`, `// 4.5`, `// 4.6`, `// 4.7`) — these are implementation notes that don't belong in the final code
- [x] 9.2 Eliminate the `BUILTIN_MAPPERS` constant in `mappers_dir.rs` — it duplicates the set of known mapper names already represented by the `MapperName` enum; derive or reference from the single source of truth instead
- [x] 9.3 Fix the `unrecognised_names_are_flagged` test in `mappers_dir.rs`: the comment "Typo: missing 's' in thingsboard" is wrong (the typo is an extra 'e' in `custom`); reduce to two cases — one with an unrecognised type prefix (`custome.thingsboard`) and one bare unrecognised name (`thingsboard`)

## 10. Code Review Fixes

- [x] 10.1 Fix stale error message in `check_startup_config` (`mapper.rs:157`): the hint says "Create a tedge.toml with [connection] settings" but task 2.1 removed the `[connection]` wrapper — update to mention the top-level `url` field instead (e.g. `url = "host:8883"`)
- [x] 10.2 Fix silent no-op when `tedge.toml` exists but neither `bridge/` nor `flows/` do: the current "do nothing" guard (`mapper.rs:180`) only triggers when `tedge.toml` is also absent, so a `tedge.toml`-only directory starts with no bridge and no flows; either error with a clear message or extend the check to cover this case, and update the design documentation to clarify how this works.
- [x] 10.3 Expand `ConnectionConfig` / `DeviceConfig` to match the full schema defined in D3: rename `connection.clean_session` → `bridge.clean_session`; add `bridge.keepalive_interval`; add `device.id` (explicit MQTT client ID, overrides cert CN); add top-level `auth_method` (`auto` | `certificate` | `password`); add top-level `credentials_path` for basic auth; update `mapper.rs` to apply these fields to `MqttOptions` accordingly
- [x] 10.4 Add a test that verifies `warn_unrecognised_mapper_dirs` actually emits warnings for unrecognised directories (the existing tests only verify no panic); use `tracing_test` or capture log output to assert that `custome.thingsboard` and `thingsboard` each trigger a warning
- [x] 10.5 Decide and implement the "Built-in mappers can use mapper namespace" spec scenario: either (a) make the built-in mappers (c8y, az, aws) pass their own config table to `load_bridge_rules_from_directory` so `${mapper.*}` works in their bridge rules, or (b) remove the scenario from the spec as out of scope for this change
- [x] 10.6 Warn (or error) when `device.cert_path` is set without `device.key_path` (or vice versa) in the custom mapper's `tedge.toml`: currently both fields are wrapped in `if let (Some(cert_path), Some(key_path)) = ...` so a half-configured TLS identity silently falls back to no client auth, which will produce a confusing cloud-side rejection; emit a clear error at startup instead
- [x] 10.7 Extract the duplicate `TemplateComponent::Mapper` expansion arm into a shared helper: `expand_config_template` and `expand_loop_template` contain identical `Mapper(path, key_span) => { … }` blocks; factor them into a small free function (e.g. `expand_mapper_component(path, key_span, whole_span, mapper_config)`) to keep the two expansion sites in sync
- [x] 10.8 Decouple `list_custom_profiles` from CLI presentation: the function currently returns strings like `"--profile thingsboard"` and `"(default, no --profile)"`, fusing data with display; return plain `Option<String>` profile names instead and format the CLI hint at the call site in the error message

## 11. `start` Method Decomposition

The `TEdgeComponent::start` implementation in `mapper.rs` mixes actor-graph wiring with business logic (auth method resolution, TLS config building, credential loading). The actor-wiring parts are hard to unit-test, but the transport-setup logic can be extracted into a pure synchronous function and tested independently.

- [x] 11.1 Extract `build_cloud_mqtt_options` from `start`: pull the block that reads `config.url`, constructs `MqttOptions`, sets clean-session and keepalive, resolves the CA path, resolves the effective auth method, and configures either certificate TLS or password credentials into a standalone synchronous function `build_cloud_mqtt_options(config: &CustomMapperConfig, service_name: &str, mapper_dir: &Utf8Path, tedge_config: &TEdgeConfig) -> anyhow::Result<(MqttOptions, AuthMethod)>`; the `configure_proxy` call can be included since it is also synchronous
- [x] 11.2 Add unit tests for `build_cloud_mqtt_options` covering the following cases: (a) `auth_method = "certificate"` with `device.cert_path` + `device.key_path` → `AuthMethod::Certificate` and TLS transport set; (b) `device.id` present → used as MQTT client ID in preference to cert CN; (c) `device.id` absent → client ID derived from cert CN; (d) `auth_method = "password"` with `credentials_path` → `AuthMethod::Password` and password transport set; (e) `auth_method = "password"` without `credentials_path` → error; (f) `auth_method = "auto"` with `credentials_path` present → resolves to `AuthMethod::Password`; (g) `auth_method = "auto"` with no `credentials_path` → resolves to `AuthMethod::Certificate`; (h) missing `url` field in config → error with file path
- [x] 11.3 Extract `build_flows_actors` from `start`: pull the flows-dir wiring block (lines building `FlowsMapperConfig`, `ConnectedFlowRegistry`, `FsWatchActorBuilder`, `WatchActorBuilder`, and calling `FlowsMapperBuilder::try_new`) into a helper `async fn build_flows_actors(mapper_dir: &Utf8Path, service_name: &str, tedge_config: &TEdgeConfig) -> anyhow::Result<(FlowsMapperBuilder, FsWatchActorBuilder, WatchActorBuilder)>`; `start` then calls `.connect` on the returned builder and spawns all three — keeping actor-graph wiring in the top-level method where it is visible
- [x] 11.4 After the extractions above, verify that `start` itself reads as a linear sequence of named steps with no deeply nested branching: validate → build transport → spawn bridge → spawn flows → run
