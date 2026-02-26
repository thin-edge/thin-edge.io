## 1. Bridge Template System Extension

- [x] 1.1 Add `mapper_config: Option<&toml::Table>` field to `TemplateContext` struct
- [x] 1.2 Register `mapper` as a recognised namespace keyword in the template parser (alongside `config` and `connection`)
- [x] 1.3 Implement `${mapper.KEY}` resolution by walking the TOML table (support nested keys like `${mapper.bridge.topic_prefix}`)
- [x] 1.4 Return a clear error when `${mapper.KEY}` is used but the key does not exist in the table
- [x] 1.5 Add unit tests for `${mapper.*}` expansion (simple key, nested key, missing key)
- [x] 1.6 Add unit test for combining `${mapper.*}` and `${config.*}` in the same template

## 2. Custom Mapper Config Parsing

- [x] 2.1 Define a `CustomMapperConfig` struct (or use `toml::Table`) for the mapper's `tedge.toml`
- [x] 2.2 Implement a parser that reads `tedge.toml` from the mapper directory and returns the typed config (or raw table for templates)
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
- [x] 4.7 Support the empty-directory case: start successfully with no active components when neither `tedge.toml` nor `flows/` is present

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
